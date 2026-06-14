//! Artifact rules (020, GDD §11.3): the end-game power-ups held in Natar villages, captured by force,
//! and applied as **read-time modifiers** (the oasis-bonus pattern). Pure (P3) — no I/O.
//!
//! Each artifact has a [`ArtifactKind`] and an [`ArtifactScope`]. Effects are aggregated from a player's
//! holdings into [`ArtifactEffects`] — per-hook multiplicative factors the sim reads combine with their
//! existing factors (speed, Smithy, oasis bonus). An absent effect is the identity `1.0`.

/// The eight faithful artifact types (GDD §11.3). Each maps to exactly one simulation hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    /// Faster troop movement (travel time).
    Speed,
    /// Larger warehouse/granary capacity.
    Storage,
    /// Reduced troop crop upkeep.
    Sustenance,
    /// Faster troop training.
    Trainer,
    /// Tougher buildings — less catapult/siege damage.
    Architect,
    /// Sharper scouting (offence).
    Eyes,
    /// Harder to scout (defence).
    Confuser,
    /// A chaotic artifact — resolves to a **seeded** concrete kind, fixed per artifact (see
    /// [`fool_resolved`]); never random at read time (P6).
    Fool,
}

/// How widely an artifact's effect reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactScope {
    /// Affects only the holding village.
    Small,
    /// Affects all the holder's villages (account-wide).
    Large,
    /// Account-wide and strongest; one per type per world.
    Unique,
}

/// A stable artifact id (catalogue key + the Fool seed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactId(pub String);

/// A released artifact: its type, scope, and effect magnitude (interpretation is per-kind — a factor
/// applied to the matching [`ArtifactEffects`] field).
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactDef {
    /// Stable id (catalogue key).
    pub id: ArtifactId,
    /// What it does.
    pub kind: ArtifactKind,
    /// How far it reaches.
    pub scope: ArtifactScope,
    /// The effect magnitude (a factor or fraction, interpreted per kind).
    pub magnitude: f64,
}

/// Per-hook multiplicative modifiers aggregated from a set of holdings. `1.0` everywhere is "no effect".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtifactEffects {
    /// Troop-speed multiplier (>1 = faster).
    pub troop_speed: f64,
    /// Storage-capacity multiplier (>1 = bigger).
    pub storage: f64,
    /// Upkeep multiplier (<1 = cheaper).
    pub upkeep: f64,
    /// Training-time multiplier (<1 = faster).
    pub training: f64,
    /// Building-durability multiplier (<1 = less siege damage taken).
    pub durability: f64,
    /// Scouting-power multiplier for this account's scouts (>1 = sharper offence).
    pub scout_power: f64,
    /// Scouting-defence multiplier (>1 = harder to scout).
    pub scout_defense: f64,
}

impl ArtifactEffects {
    /// The identity effects — no artifacts held.
    pub const NONE: ArtifactEffects = ArtifactEffects {
        troop_speed: 1.0,
        storage: 1.0,
        upkeep: 1.0,
        training: 1.0,
        durability: 1.0,
        scout_power: 1.0,
        scout_defense: 1.0,
    };
}

impl Default for ArtifactEffects {
    fn default() -> Self {
        ArtifactEffects::NONE
    }
}

/// Resolve a kind to its concrete effect kind — identity except **Fool**, which deterministically maps
/// to one of the seven non-Fool kinds from its id (P6: fixed at release, never random at read).
pub fn fool_resolved(def: &ArtifactDef) -> ArtifactKind {
    if def.kind != ArtifactKind::Fool {
        return def.kind;
    }
    const POOL: [ArtifactKind; 7] = [
        ArtifactKind::Speed,
        ArtifactKind::Storage,
        ArtifactKind::Sustenance,
        ArtifactKind::Trainer,
        ArtifactKind::Architect,
        ArtifactKind::Eyes,
        ArtifactKind::Confuser,
    ];
    // A stable hash of the id selects the pool slot — deterministic across runs (no Hasher randomness).
    let h = def.id.0.bytes().fold(0u64, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u64::from(b))
    });
    POOL[(h % POOL.len() as u64) as usize]
}

/// Fold one artifact's `(resolved kind, magnitude)` into an effects accumulator.
fn apply_to_effects(eff: &mut ArtifactEffects, def: &ArtifactDef) {
    let m = def.magnitude;
    match fool_resolved(def) {
        ArtifactKind::Speed => eff.troop_speed *= m,
        ArtifactKind::Storage => eff.storage *= m,
        ArtifactKind::Sustenance => eff.upkeep *= m,
        ArtifactKind::Trainer => eff.training *= m,
        ArtifactKind::Architect => eff.durability *= m,
        ArtifactKind::Eyes => eff.scout_power *= m,
        ArtifactKind::Confuser => eff.scout_defense *= m,
        ArtifactKind::Fool => unreachable!("fool_resolved never returns Fool"),
    }
}

/// Aggregate the effects in force for a village: its own **small**-scope holdings plus the account's
/// **large**/**unique** holdings (which reach every village). Multiple artifacts stack multiplicatively.
pub fn aggregate_effects(
    village_small: &[ArtifactDef],
    account_wide: &[ArtifactDef],
) -> ArtifactEffects {
    let mut eff = ArtifactEffects::NONE;
    for def in village_small
        .iter()
        .filter(|d| d.scope == ArtifactScope::Small)
    {
        apply_to_effects(&mut eff, def);
    }
    for def in account_wide
        .iter()
        .filter(|d| matches!(d.scope, ArtifactScope::Large | ArtifactScope::Unique))
    {
        apply_to_effects(&mut eff, def);
    }
    eff
}

/// The Treasury level required to hold an artifact of `scope` (small lowest, unique highest).
pub fn required_treasury_level(scope: ArtifactScope, small: u8, large: u8, unique: u8) -> u8 {
    match scope {
        ArtifactScope::Small => small,
        ArtifactScope::Large => large,
        ArtifactScope::Unique => unique,
    }
}

/// Whether an attacking village can capture an artifact of `required_level`: its Treasury is high enough
/// **and** it does not already hold an artifact (one vault per village).
pub fn can_capture(treasury_level: u8, required_level: u8, already_holds: bool) -> bool {
    treasury_level >= required_level && !already_holds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, kind: ArtifactKind, scope: ArtifactScope, magnitude: f64) -> ArtifactDef {
        ArtifactDef {
            id: ArtifactId(id.to_owned()),
            kind,
            scope,
            magnitude,
        }
    }

    #[test]
    fn no_holdings_is_identity() {
        assert_eq!(aggregate_effects(&[], &[]), ArtifactEffects::NONE);
    }

    #[test]
    fn small_applies_only_to_the_village_set() {
        let small = vec![def("a", ArtifactKind::Speed, ArtifactScope::Small, 2.0)];
        let eff = aggregate_effects(&small, &[]);
        assert_eq!(eff.troop_speed, 2.0);
        // A small artifact passed only in the account-wide slot does NOT apply (filtered by scope).
        let eff2 = aggregate_effects(&[], &small);
        assert_eq!(eff2.troop_speed, 1.0);
    }

    #[test]
    fn account_wide_applies_and_stacks() {
        let acct = vec![
            def("a", ArtifactKind::Storage, ArtifactScope::Large, 1.5),
            def("b", ArtifactKind::Storage, ArtifactScope::Unique, 2.0),
        ];
        let eff = aggregate_effects(&[], &acct);
        assert_eq!(eff.storage, 3.0, "large × unique stack multiplicatively");
    }

    #[test]
    fn each_kind_hits_its_own_factor() {
        let all = vec![
            def("s", ArtifactKind::Sustenance, ArtifactScope::Large, 0.75),
            def("t", ArtifactKind::Trainer, ArtifactScope::Large, 0.5),
            def("ar", ArtifactKind::Architect, ArtifactScope::Large, 0.6),
            def("e", ArtifactKind::Eyes, ArtifactScope::Large, 1.3),
            def("c", ArtifactKind::Confuser, ArtifactScope::Large, 1.4),
        ];
        let eff = aggregate_effects(&[], &all);
        assert_eq!(eff.upkeep, 0.75);
        assert_eq!(eff.training, 0.5);
        assert_eq!(eff.durability, 0.6);
        assert_eq!(eff.scout_power, 1.3);
        assert_eq!(eff.scout_defense, 1.4);
        assert_eq!(eff.troop_speed, 1.0);
        assert_eq!(eff.storage, 1.0);
    }

    #[test]
    fn fool_is_deterministic_and_never_fool() {
        let f = def("fool-1", ArtifactKind::Fool, ArtifactScope::Large, 1.5);
        let r1 = fool_resolved(&f);
        let r2 = fool_resolved(&f);
        assert_eq!(r1, r2, "same id ⇒ same resolution");
        assert_ne!(r1, ArtifactKind::Fool);
        // It applies to whichever factor it resolved to — exactly one factor moves off identity.
        let eff = aggregate_effects(&[], &[f]);
        let moved = [
            eff.troop_speed,
            eff.storage,
            eff.upkeep,
            eff.training,
            eff.durability,
            eff.scout_power,
            eff.scout_defense,
        ]
        .iter()
        .filter(|&&x| (x - 1.0).abs() > f64::EPSILON)
        .count();
        assert_eq!(moved, 1, "the Fool moved exactly one factor");
    }

    #[test]
    fn capture_requires_treasury_and_empty_vault() {
        assert_eq!(
            required_treasury_level(ArtifactScope::Small, 10, 15, 20),
            10
        );
        assert_eq!(
            required_treasury_level(ArtifactScope::Unique, 10, 15, 20),
            20
        );
        assert!(
            can_capture(10, 10, false),
            "treasury high enough, vault empty"
        );
        assert!(!can_capture(9, 10, false), "treasury too low");
        assert!(!can_capture(20, 10, true), "already holding an artifact");
    }
}
