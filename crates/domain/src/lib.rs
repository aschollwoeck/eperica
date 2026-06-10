//! Eperica domain layer — the pure game core.
//!
//! Holds entities, value objects, and game rules. Per the project constitution (**P3**) this crate
//! has **no I/O, framework, or database dependencies** and is unit-testable in isolation. Game
//! modules (resources, villages, combat, …) are introduced in later slices.
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    /// Smoke test proving the workspace compiles and the test harness runs.
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
