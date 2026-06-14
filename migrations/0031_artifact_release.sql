-- Slice 020: the world's artifact-release instant (GDD §13.2 end-game schedule). NULL means "no
-- release scheduled"; set at world creation from config. The one-time release (020) materializes the
-- Natar villages + artifacts at/after this instant.
ALTER TABLE worlds ADD COLUMN artifact_release_at timestamptz;
