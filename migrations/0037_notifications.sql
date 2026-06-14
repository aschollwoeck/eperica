-- Slice 026: notifications & alerts. A persisted, per-player feed of attention-critical events (incoming
-- attack, battle report, new message), delivered live via LISTEN/NOTIFY on the 'notifications' channel.

CREATE TABLE notifications (
    id         uuid PRIMARY KEY,
    world_id   uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    -- The recipient. A notification is strictly private to this player (026 AC7).
    player_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Stable kind token (domain NotificationKind::as_str): 'incoming_attack' / 'battle_report' / 'new_message'.
    kind       text NOT NULL,
    -- Optional pointer the UI dereferences: ref_kind in {'report','dm','village'}, ref_id its target
    -- (a report uuid, the other party's uuid, or "x|y" coordinates). Null for kinds with no deep link.
    ref_kind   text,
    ref_id     text,
    -- A short pre-rendered detail line (e.g. arrival time / coords); kept denormalised so the feed read is
    -- a single table scan with no joins (P11).
    body       text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    -- Per-recipient read watermark; NULL = unread.
    read_at    timestamptz
);

-- The feed: a player's notifications, most-recent first (bounded page).
CREATE INDEX notifications_feed ON notifications (player_id, created_at DESC);
-- The unread count (the nav bell): a partial index over just the unread rows.
CREATE INDEX notifications_unread ON notifications (player_id) WHERE read_at IS NULL;
