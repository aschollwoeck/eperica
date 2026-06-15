-- Administrator role (036 — M9 multi-world & administration).
-- Additive to Player/Moderator: an admin operates worlds and accounts. Bootstrapped from the ADMINS env
-- (mirroring MODERATORS / 0034 fair-play) and grantable in-app from the /admin console.
ALTER TABLE users
    ADD COLUMN is_admin boolean NOT NULL DEFAULT false;
