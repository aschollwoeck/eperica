-- Slice 014 (T1): per-village loyalty (GDD §3.4, §9.4 step 5). Loyalty is a value in [0, 100] that
-- regenerates toward 100 over time; an administrator that survives a won battle lowers it, and at zero
-- the village is conquered (ownership transfers). Lazy, like resources (002) and culture (013): stored
-- as value + lastUpdated, regenerated on read; re-anchored when an administrator strike changes it.
-- A fresh or pre-014 village starts fully loyal (the column default also seeds every existing row).

ALTER TABLE villages
    ADD COLUMN loyalty smallint NOT NULL DEFAULT 100,
    ADD COLUMN loyalty_updated_at timestamptz NOT NULL DEFAULT now();
