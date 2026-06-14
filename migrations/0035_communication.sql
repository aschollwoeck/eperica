-- Slice 024: communication — WhatsApp-style conversations. Direct-message threads + group chat channels,
-- all persisted (durable history + a moderation trail) and delivered live via LISTEN/NOTIFY on top.

-- Direct messages: a DM thread between A and B is every row with {sender, recipient} = {A, B}. No subject,
-- no per-side delete — a running conversation.
CREATE TABLE direct_messages (
    id           uuid PRIMARY KEY,
    world_id     uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    sender_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body         text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- Both access directions: history of a thread reads rows in either direction; the conversations list groups
-- by the other party.
CREATE INDEX direct_messages_recipient ON direct_messages (recipient_id, sender_id, created_at DESC);
CREATE INDEX direct_messages_sender ON direct_messages (sender_id, recipient_id, created_at DESC);

-- Channel chat: 'global' or 'alliance:<id>'. The (channel, created_at) index serves history backfill.
CREATE TABLE chat_messages (
    id         uuid PRIMARY KEY,
    world_id   uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    channel    text NOT NULL,
    sender_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body       text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX chat_messages_channel ON chat_messages (channel, created_at DESC);

-- Per-viewer, per-conversation read watermark. `conversation` is the viewer-relative key: 'dm:<other>',
-- 'global', or 'alliance:<id>'. Unread = messages after last_read_at not sent by the viewer.
CREATE TABLE conversation_reads (
    player_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation text NOT NULL,
    last_read_at timestamptz NOT NULL,
    PRIMARY KEY (player_id, conversation)
);
