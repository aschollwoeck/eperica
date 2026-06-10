-- Per-village stored resource amounts (slice 002). Integer units; settled lazily, computed on read.

CREATE TABLE village_resources (
    village_id uuid PRIMARY KEY REFERENCES villages(id) ON DELETE CASCADE,
    wood       bigint NOT NULL,
    clay       bigint NOT NULL,
    iron       bigint NOT NULL,
    crop       bigint NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
