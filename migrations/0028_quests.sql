-- Slice 018: onboarding quest chain — once-only completions per player.
-- The PK (player, quest) is the exactly-once guard (a quest + its reward apply at most once). The
-- player's *current* quest is derived (the first chain entry not present here); there is no
-- in-progress row.
CREATE TABLE player_quests (
    player_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    quest_id     text NOT NULL,
    completed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (player_id, quest_id)
);
