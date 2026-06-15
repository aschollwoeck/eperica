-- Slice 029: notification preferences. A row here means the player has **muted** that notification kind —
-- a muted kind is never recorded for them (026 generation is gated on the absence of a row). Default-on:
-- no row = enabled, so new accounts need no backfill.

CREATE TABLE notification_mutes (
    player_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Stable kind token (domain NotificationKind::as_str).
    kind      text NOT NULL,
    PRIMARY KEY (player_id, kind)
);
