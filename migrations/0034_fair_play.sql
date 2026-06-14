-- Slice 022: fair play & anti-cheat — the Moderator role, account sanctions, player reports, and the
-- DB-backed rate-limit counters that also feed the inhuman-action-rate detection signal.

-- Account capability + sanction state. `is_moderator` is the elevated role (additive to Player).
-- `suspended_until`/`banned_at` are the sanction state read on every login + action (computed on read,
-- P1): banned ⇒ always blocked, suspended ⇒ blocked while now < suspended_until. `registration_ip` is
-- captured at register and is the shared-IP detection key.
ALTER TABLE users
    ADD COLUMN is_moderator    boolean NOT NULL DEFAULT false,
    ADD COLUMN suspended_until timestamptz,
    ADD COLUMN banned_at       timestamptz,
    ADD COLUMN registration_ip text;

CREATE INDEX users_registration_ip_idx ON users (registration_ip);

-- Player reports against an account. `status` is 'open' | 'resolved'; a partial-unique index collapses a
-- duplicate **open** report by the same reporter against the same subject (no queue spam). Resolution
-- records the acting moderator + when + the outcome text.
CREATE TABLE reports (
    id          uuid PRIMARY KEY,
    world_id    uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    reporter_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    subject_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason      text NOT NULL,
    note        text NOT NULL DEFAULT '',
    status      text NOT NULL DEFAULT 'open',
    created_at  timestamptz NOT NULL DEFAULT now(),
    resolved_by uuid REFERENCES users(id) ON DELETE SET NULL,
    resolved_at timestamptz,
    resolution  text
);

CREATE UNIQUE INDEX reports_one_open_per_pair
    ON reports (reporter_id, subject_id) WHERE status = 'open';
CREATE INDEX reports_open_queue ON reports (created_at) WHERE status = 'open';

-- Fixed-window rate-limit counters (DB-backed so the web tier stays stateless + horizontally scalable,
-- P5). `subject` is a player id or, pre-auth, an IP; `action` namespaces the limit (e.g. 'action',
-- 'login'). The per-window tallies also feed the inhuman-action-rate detection signal (022 AC7).
CREATE TABLE rate_limits (
    subject      text NOT NULL,
    action       text NOT NULL,
    window_start timestamptz NOT NULL,
    count        integer NOT NULL DEFAULT 0,
    PRIMARY KEY (subject, action, window_start)
);

CREATE INDEX rate_limits_subject_action ON rate_limits (subject, action);
