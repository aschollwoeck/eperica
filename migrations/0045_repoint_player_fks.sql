-- Re-point the per-world game foreign keys from users(id) to players(id) (042 — the 037-deferred
-- switch-over). A player's id equals its user's id in the home world (037 backfill) and for the Natar NPC
-- (042 reuse-UUID), so every existing value already exists in players and these constraints hold without
-- touching data. This lets a *second* world's player (a fresh id, not a user id) own villages, movements,
-- culture, alliance membership, etc.
--
-- Account-level tables (sitting, messaging, fair-play reports, notification settings/mutes) intentionally
-- stay keyed on users(id): they belong to the account, not a per-world player.
--
-- ON DELETE actions are preserved per column (CASCADE where the original cascaded; RESTRICT otherwise).
-- players cascades from users, so a user delete still cascades transitively.

-- villages
ALTER TABLE villages DROP CONSTRAINT villages_owner_id_fkey;
ALTER TABLE villages ADD CONSTRAINT villages_owner_id_fkey
    FOREIGN KEY (owner_id) REFERENCES players(id);

-- movements / trade
ALTER TABLE troop_movements DROP CONSTRAINT troop_movements_owner_id_fkey;
ALTER TABLE troop_movements ADD CONSTRAINT troop_movements_owner_id_fkey
    FOREIGN KEY (owner_id) REFERENCES players(id);
ALTER TABLE trade_movements DROP CONSTRAINT trade_movements_owner_id_fkey;
ALTER TABLE trade_movements ADD CONSTRAINT trade_movements_owner_id_fkey
    FOREIGN KEY (owner_id) REFERENCES players(id);

-- combat reports
ALTER TABLE battle_reports DROP CONSTRAINT battle_reports_attacker_player_fkey;
ALTER TABLE battle_reports ADD CONSTRAINT battle_reports_attacker_player_fkey
    FOREIGN KEY (attacker_player) REFERENCES players(id);
ALTER TABLE battle_reports DROP CONSTRAINT battle_reports_defender_player_fkey;
ALTER TABLE battle_reports ADD CONSTRAINT battle_reports_defender_player_fkey
    FOREIGN KEY (defender_player) REFERENCES players(id);
ALTER TABLE battle_defenders DROP CONSTRAINT battle_defenders_player_id_fkey;
ALTER TABLE battle_defenders ADD CONSTRAINT battle_defenders_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id);

-- scouting
ALTER TABLE scout_reports DROP CONSTRAINT scout_reports_scouter_player_fkey;
ALTER TABLE scout_reports ADD CONSTRAINT scout_reports_scouter_player_fkey
    FOREIGN KEY (scouter_player) REFERENCES players(id);
ALTER TABLE scout_reports DROP CONSTRAINT scout_reports_target_player_fkey;
ALTER TABLE scout_reports ADD CONSTRAINT scout_reports_target_player_fkey
    FOREIGN KEY (target_player) REFERENCES players(id);

-- culture (CASCADE)
ALTER TABLE player_culture DROP CONSTRAINT player_culture_player_id_fkey;
ALTER TABLE player_culture ADD CONSTRAINT player_culture_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id) ON DELETE CASCADE;

-- alliances
ALTER TABLE alliances DROP CONSTRAINT alliances_founder_id_fkey;
ALTER TABLE alliances ADD CONSTRAINT alliances_founder_id_fkey
    FOREIGN KEY (founder_id) REFERENCES players(id);
ALTER TABLE alliance_members DROP CONSTRAINT alliance_members_player_id_fkey;
ALTER TABLE alliance_members ADD CONSTRAINT alliance_members_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id) ON DELETE CASCADE;
ALTER TABLE alliance_invitations DROP CONSTRAINT alliance_invitations_invitee_id_fkey;
ALTER TABLE alliance_invitations ADD CONSTRAINT alliance_invitations_invitee_id_fkey
    FOREIGN KEY (invitee_id) REFERENCES players(id) ON DELETE CASCADE;

-- ranking / achievements (CASCADE where original cascaded; medals.subject_id is polymorphic — no FK).
ALTER TABLE population_snapshots DROP CONSTRAINT population_snapshots_player_id_fkey;
ALTER TABLE population_snapshots ADD CONSTRAINT population_snapshots_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id) ON DELETE CASCADE;
ALTER TABLE player_achievements DROP CONSTRAINT player_achievements_player_id_fkey;
ALTER TABLE player_achievements ADD CONSTRAINT player_achievements_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id) ON DELETE CASCADE;

-- quests (CASCADE)
ALTER TABLE player_quests DROP CONSTRAINT player_quests_player_id_fkey;
ALTER TABLE player_quests ADD CONSTRAINT player_quests_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id) ON DELETE CASCADE;

-- notifications (CASCADE) — per-world game alerts go to the world player
ALTER TABLE notifications DROP CONSTRAINT notifications_player_id_fkey;
ALTER TABLE notifications ADD CONSTRAINT notifications_player_id_fkey
    FOREIGN KEY (player_id) REFERENCES players(id) ON DELETE CASCADE;
