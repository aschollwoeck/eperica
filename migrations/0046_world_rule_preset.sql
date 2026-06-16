-- Per-world rule preset (049, ADR 0035): the named rule bundle a world plays under. Defaults to the
-- existing balance ('classic'), so every existing/boot/admin world is unchanged. 050 wires the registry to
-- serve the world's preset; 052 lets an admin pick one on world creation.
ALTER TABLE worlds ADD COLUMN rule_preset text NOT NULL DEFAULT 'classic';
