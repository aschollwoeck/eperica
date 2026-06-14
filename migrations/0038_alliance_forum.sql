-- Slice 027: alliance forum — alliance-scoped threaded discussion. Threads + posts, persisted; reads and
-- writes are gated by alliance membership in the application layer (P4).

CREATE TABLE alliance_threads (
    id           uuid PRIMARY KEY,
    world_id     uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    alliance_id  uuid NOT NULL REFERENCES alliances(id) ON DELETE CASCADE,
    author_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title        text NOT NULL,
    -- A one-way announcement: locked to replies; only an Announce-right holder may start one (027 AC4).
    announcement boolean NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    -- Most-recent activity (the first post, then each reply) — drives the thread-list ordering.
    last_post_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE alliance_posts (
    id         uuid PRIMARY KEY,
    world_id   uuid NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    thread_id  uuid NOT NULL REFERENCES alliance_threads(id) ON DELETE CASCADE,
    author_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body       text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- The thread list: an alliance's threads, most-recent activity first.
CREATE INDEX alliance_threads_list ON alliance_threads (alliance_id, last_post_at DESC);
-- A thread's posts, oldest first.
CREATE INDEX alliance_posts_thread ON alliance_posts (thread_id, created_at);
