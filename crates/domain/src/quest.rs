//! Quest & onboarding rules (018): the stage-gated quest chain — pure predicates over a player's
//! persisted state, the chain walk, and the current-quest derivation. Pure (P3) — no I/O. The chain
//! content and rewards are balance (P7); detecting state and persisting completions is infra/application.

use crate::building::BuildingKind;
use crate::economy::ResourceAmounts;
use crate::units::UnitId;
use std::collections::{HashMap, HashSet};

/// A quest's stable id (also the persisted completion key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuestId(pub String);

/// What a quest requires — a predicate over the player's persisted state (018 §12.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestCondition {
    /// A resource field reaches `level` (the max field level across the player's villages).
    FieldLevel(u8),
    /// A center building of `kind` reaches `level`.
    BuildingLevel(BuildingKind, u8),
    /// The player's garrison holds any troops.
    TrainTroops,
    /// The player has launched any attack/raid.
    SendRaid,
    /// Total population reaches `threshold`.
    Population(i64),
}

/// A quest's one-time reward (any combination): resources to the capital, culture points, and/or a
/// troop count added to the capital's garrison.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestReward {
    pub resources: ResourceAmounts,
    pub culture: i64,
    pub troops: Option<(UnitId, u32)>,
}

/// One quest in the chain: its id, a human description, the completion condition, and the reward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestDef {
    pub id: QuestId,
    pub description: String,
    pub condition: QuestCondition,
    pub reward: QuestReward,
}

/// The player's persisted facts the conditions read (gathered by the application from the DB).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestProgress {
    pub max_field_level: u8,
    pub building_levels: HashMap<BuildingKind, u8>,
    pub has_troops: bool,
    pub has_raided: bool,
    pub population: i64,
}

/// Whether `progress` satisfies `condition` (P3 — pure, deterministic).
pub fn quest_met(condition: &QuestCondition, progress: &QuestProgress) -> bool {
    match condition {
        QuestCondition::FieldLevel(level) => progress.max_field_level >= *level,
        QuestCondition::BuildingLevel(kind, level) => progress
            .building_levels
            .get(kind)
            .is_some_and(|l| l >= level),
        QuestCondition::TrainTroops => progress.has_troops,
        QuestCondition::SendRaid => progress.has_raided,
        QuestCondition::Population(threshold) => progress.population >= *threshold,
    }
}

/// The player's **current** quest: the first chain entry they have not completed (`None` ⇒ the chain is
/// finished — quests have tapered off, AC7).
pub fn current_quest<'a>(
    chain: &'a [QuestDef],
    completed: &HashSet<QuestId>,
) -> Option<&'a QuestDef> {
    chain.iter().find(|q| !completed.contains(&q.id))
}

/// The quests newly completable now (AC1/AC6): walking the chain in order, the **consecutive**
/// not-completed quests whose condition is met, **stopping at the first unmet** (the stage gate). So a
/// later quest never completes before an earlier one, and a player whose state already satisfies a
/// prefix completes that prefix in order.
pub fn newly_completed<'a>(
    chain: &'a [QuestDef],
    completed: &HashSet<QuestId>,
    progress: &QuestProgress,
) -> Vec<&'a QuestDef> {
    let mut out = Vec::new();
    for q in chain {
        if completed.contains(&q.id) {
            continue;
        }
        if quest_met(&q.condition, progress) {
            out.push(q);
        } else {
            break; // the stage gate — quests after the first unmet are not yet reachable
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest(id: &str, condition: QuestCondition) -> QuestDef {
        QuestDef {
            id: QuestId(id.to_owned()),
            description: id.to_owned(),
            condition,
            reward: QuestReward::default(),
        }
    }

    fn chain() -> Vec<QuestDef> {
        vec![
            quest("a", QuestCondition::FieldLevel(2)),
            quest(
                "b",
                QuestCondition::BuildingLevel(BuildingKind::Warehouse, 1),
            ),
            quest("c", QuestCondition::TrainTroops),
        ]
    }

    #[test]
    fn quest_met_per_condition() {
        let p = QuestProgress {
            max_field_level: 2,
            building_levels: HashMap::from([(BuildingKind::Warehouse, 1)]),
            has_troops: true,
            has_raided: false,
            population: 40,
        };
        assert!(quest_met(&QuestCondition::FieldLevel(2), &p));
        assert!(!quest_met(&QuestCondition::FieldLevel(3), &p));
        assert!(quest_met(
            &QuestCondition::BuildingLevel(BuildingKind::Warehouse, 1),
            &p
        ));
        assert!(!quest_met(
            &QuestCondition::BuildingLevel(BuildingKind::Granary, 1),
            &p
        ));
        assert!(quest_met(&QuestCondition::TrainTroops, &p));
        assert!(!quest_met(&QuestCondition::SendRaid, &p));
        assert!(quest_met(&QuestCondition::Population(40), &p));
        assert!(!quest_met(&QuestCondition::Population(41), &p));
    }

    #[test]
    fn chain_is_stage_gated() {
        let chain = chain();
        let completed = HashSet::new();
        // c's condition is met but a/b are not → nothing completes (can't skip ahead).
        let p = QuestProgress {
            has_troops: true,
            ..QuestProgress::default()
        };
        assert!(newly_completed(&chain, &completed, &p).is_empty());
        assert_eq!(current_quest(&chain, &completed).unwrap().id.0, "a");
    }

    #[test]
    fn resumable_prefix_completes_in_order() {
        let chain = chain();
        let completed = HashSet::new();
        // a and b satisfied, c not → both complete in order, stop at c.
        let p = QuestProgress {
            max_field_level: 2,
            building_levels: HashMap::from([(BuildingKind::Warehouse, 1)]),
            has_troops: false,
            ..QuestProgress::default()
        };
        let done: Vec<_> = newly_completed(&chain, &completed, &p)
            .iter()
            .map(|q| q.id.0.clone())
            .collect();
        assert_eq!(done, vec!["a", "b"]);
    }

    #[test]
    fn tapers_when_all_completed() {
        let chain = chain();
        let completed: HashSet<QuestId> = chain.iter().map(|q| q.id.clone()).collect();
        assert!(current_quest(&chain, &completed).is_none());
        let p = QuestProgress {
            max_field_level: 9,
            has_troops: true,
            ..QuestProgress::default()
        };
        assert!(newly_completed(&chain, &completed, &p).is_empty());
    }
}
