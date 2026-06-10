-- Initial schema (slice 001 — foundation).
-- The session table is managed by the tower-sessions Postgres store (created at startup), not here.
-- All identifiers are application-generated UUIDs; all timestamps are timestamptz (UTC, P11).

CREATE TABLE worlds (
    id         uuid PRIMARY KEY,
    speed      double precision NOT NULL,
    radius     integer NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id              uuid PRIMARY KEY,
    username        text NOT NULL UNIQUE,
    email           text NOT NULL UNIQUE,
    password_hash   text NOT NULL,
    email_confirmed boolean NOT NULL DEFAULT false,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE villages (
    id         uuid PRIMARY KEY,
    world_id   uuid NOT NULL REFERENCES worlds(id),
    owner_id   uuid NOT NULL REFERENCES users(id),
    x          integer NOT NULL,
    y          integer NOT NULL,
    tribe      text,
    created_at timestamptz NOT NULL DEFAULT now(),
    -- No two villages may share a tile in a world (AC3).
    UNIQUE (world_id, x, y)
);

CREATE INDEX villages_owner_idx ON villages (owner_id);

CREATE TABLE village_fields (
    village_id    uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    slot          smallint NOT NULL,
    resource_type text NOT NULL,
    level         smallint NOT NULL,
    PRIMARY KEY (village_id, slot)
);

CREATE TABLE village_buildings (
    village_id    uuid NOT NULL REFERENCES villages(id) ON DELETE CASCADE,
    slot          smallint NOT NULL,
    building_type text NOT NULL,
    level         smallint NOT NULL,
    PRIMARY KEY (village_id, slot)
);

CREATE TABLE scheduled_events (
    id         uuid PRIMARY KEY,
    kind       text NOT NULL,
    payload    jsonb NOT NULL DEFAULT '{}'::jsonb,
    due_at     timestamptz NOT NULL,
    seq        bigserial NOT NULL,
    status     text NOT NULL DEFAULT 'pending',
    created_at timestamptz NOT NULL DEFAULT now()
);

-- The scheduler selects pending events ordered by (due_at, seq); seq breaks same-instant ties
-- deterministically (P11).
CREATE INDEX scheduled_events_due_idx ON scheduled_events (status, due_at, seq);
