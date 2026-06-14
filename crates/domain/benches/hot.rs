//! Criterion micro-benchmarks for the pure hot domain functions (023 AC7): combat resolution, the
//! economy compute-on-read, and travel time. Re-runnable via `cargo bench -p eperica-domain`.
//!
//! These cover the per-event / per-read CPU cost that the lazy model (P1) pays on every settle and every
//! battle — the inner loops behind the latency budgets. They use inline fixtures (the domain crate has no
//! balance loader) representative of mid-game values.

use criterion::{Criterion, criterion_group, criterion_main};
use eperica_domain::{
    AttackMode, AttackPower, BattleInput, BuildingKind, BuildingSlot, CombatRules, EconomyRules,
    GameSpeed, OasisBonus, ResourceAmounts, ResourceField, ResourceKind, Tribe, WallProfile,
    compute_economy, resolve_battle, travel_time_secs,
};
use std::collections::HashMap;
use std::hint::black_box;

fn combat_rules() -> CombatRules {
    let wall = |bonus: f64, ram: f64| WallProfile {
        bonus_per_level: (0..=20).map(|l| f64::from(l) * bonus).collect(),
        ram_durability: ram,
    };
    CombatRules {
        loss_exponent: 1.5,
        luck_range: 0.25,
        morale_exponent: 0.2,
        base_defense: 10.0,
        smithy_bonus_per_level: 0.015,
        catapult_durability: 100.0,
        cranny_bypass_teuton: 0.5,
        cranny_protection_per_level: vec![0, 1000, 2000],
        walls: HashMap::from([
            (Tribe::Gauls, wall(0.03, 100.0)),
            (Tribe::Romans, wall(0.03, 100.0)),
            (Tribe::Teutons, wall(0.02, 200.0)),
        ]),
    }
}

fn economy_rules() -> EconomyRules {
    EconomyRules {
        wood_per_level: (0..=20).map(|l| i64::from(l) * 30).collect(),
        clay_per_level: (0..=20).map(|l| i64::from(l) * 30).collect(),
        iron_per_level: (0..=20).map(|l| i64::from(l) * 30).collect(),
        crop_per_level: (0..=20).map(|l| i64::from(l) * 30).collect(),
        field_population_per_level: (0..=20).map(i64::from).collect(),
        building_population_per_level: HashMap::from([
            (
                BuildingKind::MainBuilding,
                (0..=20).map(i64::from).collect(),
            ),
            (BuildingKind::Warehouse, (0..=20).map(i64::from).collect()),
            (BuildingKind::Granary, (0..=20).map(i64::from).collect()),
        ]),
        warehouse_capacity_per_level: (0..=20).map(|l| 800 + i64::from(l) * 1000).collect(),
        granary_capacity_per_level: (0..=20).map(|l| 800 + i64::from(l) * 1000).collect(),
        outpost_capacity_per_level: vec![0, 1, 2, 3],
        starting_amounts: ResourceAmounts {
            wood: 750,
            clay: 750,
            iron: 750,
            crop: 750,
        },
    }
}

fn bench_combat(c: &mut Criterion) {
    let rules = combat_rules();
    let input = BattleInput {
        attack: AttackPower {
            infantry: 5000.0,
            cavalry: 3000.0,
            ram: 200.0,
        },
        def_infantry: 4000.0,
        def_cavalry: 2500.0,
        wall_tribe: Tribe::Gauls,
        wall_level: 10,
        attacker_pop: 800,
        defender_pop: 750,
    };
    c.bench_function("resolve_battle", |b| {
        b.iter(|| {
            resolve_battle(
                black_box(AttackMode::Attack),
                black_box(input),
                black_box(&rules),
                black_box(0.5),
            )
        })
    });
}

fn bench_economy(c: &mut Criterion) {
    let rules = economy_rules();
    // A full 18-field village + a handful of centre buildings — the per-settle compute.
    let fields: Vec<ResourceField> = (0..18)
        .map(|i| ResourceField {
            kind: match i % 4 {
                0 => ResourceKind::Wood,
                1 => ResourceKind::Clay,
                2 => ResourceKind::Iron,
                _ => ResourceKind::Crop,
            },
            level: 8,
        })
        .collect();
    let buildings = vec![
        BuildingSlot {
            kind: BuildingKind::MainBuilding,
            level: 12,
        },
        BuildingSlot {
            kind: BuildingKind::Warehouse,
            level: 14,
        },
        BuildingSlot {
            kind: BuildingKind::Granary,
            level: 14,
        },
    ];
    let stored = ResourceAmounts {
        wood: 5000,
        clay: 5000,
        iron: 5000,
        crop: 5000,
    };
    c.bench_function("compute_economy", |b| {
        b.iter(|| {
            compute_economy(
                black_box(stored),
                black_box(3600),
                black_box(&fields),
                black_box(&buildings),
                black_box(120),
                black_box(&rules),
                black_box(GameSpeed::new(1.0).unwrap()),
                black_box(OasisBonus::default()),
                black_box(1.0),
            )
        })
    });
}

fn bench_travel(c: &mut Criterion) {
    let speed = GameSpeed::new(1.0).unwrap();
    c.bench_function("travel_time_secs", |b| {
        b.iter(|| travel_time_secs(black_box(42.0), black_box(6), black_box(speed)))
    });
}

criterion_group!(benches, bench_combat, bench_economy, bench_travel);
criterion_main!(benches);
