//! Ports — the capabilities the application needs from the outside world.
//!
//! These traits are implemented by the infrastructure layer (databases, password hashing, …). Keeping
//! them here lets use-cases be written and tested against fakes, with no I/O dependency.

use async_trait::async_trait;
use eperica_domain::{
    BuildTarget, EventKind, PlayerId, QueueLane, ResourceAmounts, StartingVillage, Timestamp,
    Tribe, UnitId, Village, VillageId,
};

/// Details for a new account to be created.
#[derive(Debug, Clone)]
pub struct NewUser {
    /// Unique login name.
    pub username: String,
    /// Unique email address.
    pub email: String,
    /// Already-hashed password (the application never stores plaintext).
    pub password_hash: String,
    /// Whether the account is considered email-confirmed at creation.
    pub email_confirmed: bool,
    /// The tribe chosen at registration (immutable thereafter, 004 AC1/AC2).
    pub tribe: Tribe,
}

/// A persisted account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRecord {
    /// Stable identity.
    pub id: PlayerId,
    /// Login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Stored password hash.
    pub password_hash: String,
    /// Whether the email has been confirmed.
    pub email_confirmed: bool,
    /// The account's tribe (chosen at registration; pre-004 accounts were backfilled).
    pub tribe: Tribe,
}

/// Errors a repository/port can return to the application.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// A uniqueness constraint was violated (e.g. duplicate username or email).
    #[error("a unique constraint was violated")]
    Duplicate,
    /// The state the caller computed from changed concurrently (optimistic check failed); the
    /// operation was not applied and can be retried from a fresh read.
    #[error("the state changed concurrently; retry")]
    Conflict,
    /// No free tile remained to place a starting village.
    #[error("the world is full")]
    WorldFull,
    /// A backend/storage failure.
    #[error("storage error: {0}")]
    Backend(String),
}

/// Hashes and verifies passwords. (Synchronous: hashing is CPU-bound, not I/O.)
pub trait PasswordHasher: Send + Sync {
    /// Hash a plaintext password for storage.
    ///
    /// # Errors
    /// Returns [`RepoError`] if hashing fails.
    fn hash(&self, password: &str) -> Result<String, RepoError>;

    /// Verify a plaintext password against a stored hash.
    ///
    /// # Errors
    /// Returns [`RepoError`] if the stored hash cannot be parsed.
    fn verify(&self, password: &str, hash: &str) -> Result<bool, RepoError>;
}

/// Persistence for accounts and their villages.
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Atomically create the user **and** their starting village (a single transaction), placing the
    /// village on the first free in-bounds tile. Returns the created user.
    ///
    /// # Errors
    /// [`RepoError::Duplicate`] if the username/email is taken; [`RepoError::WorldFull`] if no tile is
    /// free; [`RepoError::Backend`] on storage failure.
    async fn create_account(
        &self,
        user: NewUser,
        template: &StartingVillage,
    ) -> Result<UserRecord, RepoError>;

    /// Look up a user by login name.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn find_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, RepoError>;

    /// Look up a user by id.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError>;

    /// All villages owned by a player (with their fields and buildings).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn villages_of(&self, owner: PlayerId) -> Result<Vec<Village>, RepoError>;

    /// A village's stored resource amounts and the time they were last settled (Unix-ms UTC).
    /// Resources accrue from this snapshot on read (P1); there is no background job.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn stored_resources(
        &self,
        village: VillageId,
    ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError>;
}

/// A claimed, due event ready to be processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueEvent {
    /// The event's identity (128-bit; mapped to a UUID by the infrastructure).
    pub id: u128,
    /// What should happen.
    pub kind: EventKind,
    /// When it became due (Unix-ms, UTC).
    pub due_at: Timestamp,
}

/// Persistence and claiming of scheduled, due-timestamped events (P1).
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Persist a new pending event due at `due_at`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn schedule(&self, kind: EventKind, due_at: Timestamp) -> Result<(), RepoError>;

    /// Atomically claim up to `limit` due events (status `pending` → `processing`), nearest-due
    /// first by `(due_at, seq)` so same-instant order is deterministic (P11). Claiming is exclusive
    /// across workers (no event is processed twice).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due(&self, now: Timestamp, limit: i64) -> Result<Vec<DueEvent>, RepoError>;

    /// Mark a claimed event as processed.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn mark_done(&self, id: u128) -> Result<(), RepoError>;
}

/// A new build order to enqueue.
#[derive(Debug, Clone, Copy)]
pub struct NewBuildOrder {
    /// What is being built/upgraded.
    pub target: BuildTarget,
    /// The level the target reaches on completion.
    pub target_level: u8,
    /// When the order completes (Unix-ms UTC).
    pub complete_at: Timestamp,
    /// The queue lane the order occupies (the Roman trait, 004 AC13) — computed server-side.
    pub lane: QueueLane,
}

/// A village's currently-active (pending) build order.
#[derive(Debug, Clone, Copy)]
pub struct ActiveBuild {
    /// What is building.
    pub target: BuildTarget,
    /// The level it reaches.
    pub target_level: u8,
    /// Completion time (Unix-ms UTC).
    pub complete_at: Timestamp,
}

/// A claimed, due build order ready to apply.
#[derive(Debug, Clone, Copy)]
pub struct DueBuild {
    /// Order identity.
    pub id: u128,
    /// The village it belongs to.
    pub village: VillageId,
    /// What to apply.
    pub target: BuildTarget,
    /// The level to set.
    pub target_level: u8,
}

/// Persistence for the build queue (due-timestamped orders, P1).
#[async_trait]
pub trait BuildRepository: Send + Sync {
    /// Atomically settle the village's resources to `settled` (at `now`) and enqueue `order`. The
    /// one-active-order-per-lane rule is enforced by storage (non-Romans share one lane; Romans
    /// get a field and a building lane, 004 AC13); a conflicting active order returns
    /// [`RepoError::Duplicate`].
    ///
    /// `settled` was computed from the snapshot read at `settled_from` (the resources row's
    /// last-settled time); the settle applies **only if the row is still at that snapshot**,
    /// otherwise [`RepoError::Conflict`] — so concurrent orders on different queues can never
    /// overwrite each other's debit (P2/P4).
    ///
    /// # Errors
    /// [`RepoError`] on conflict or storage failure.
    async fn start_build(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        order: NewBuildOrder,
    ) -> Result<(), RepoError>;

    /// The village's active (pending) orders — at most one per lane (so at most two, for Romans).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn active_builds(&self, village: VillageId) -> Result<Vec<ActiveBuild>, RepoError>;

    /// Atomically claim up to `limit` due orders (`pending` → `processing`), nearest-due first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_builds(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueBuild>, RepoError>;

    /// Apply a claimed order (set the target's level) and mark it done (idempotent).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_build(&self, due: DueBuild) -> Result<(), RepoError>;
}

/// Which per-village unit queue an order occupies (004): each kind allows **one** active order per
/// village, independently of the other and of the construction queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitOrderKind {
    /// Academy research of a unit type (AC6).
    Research,
    /// Smithy upgrade of a researched unit type by one level (AC10).
    SmithyUpgrade,
}

/// A new research/upgrade order to enqueue.
#[derive(Debug, Clone)]
pub struct NewUnitOrder {
    /// Which queue this order occupies.
    pub kind: UnitOrderKind,
    /// The unit type being researched/upgraded.
    pub unit: UnitId,
    /// The level reached on completion (Smithy upgrades); `None` for research.
    pub target_level: Option<u8>,
    /// When the order completes (Unix-ms UTC).
    pub complete_at: Timestamp,
}

/// A village's currently-active (pending) research/upgrade order.
#[derive(Debug, Clone)]
pub struct ActiveUnitOrder {
    /// Which queue the order occupies.
    pub kind: UnitOrderKind,
    /// The unit type being researched/upgraded.
    pub unit: UnitId,
    /// The level reached on completion (Smithy upgrades); `None` for research.
    pub target_level: Option<u8>,
    /// Completion time (Unix-ms UTC).
    pub complete_at: Timestamp,
}

/// A claimed, due research/upgrade order ready to apply.
#[derive(Debug, Clone)]
pub struct DueUnitOrder {
    /// Order identity.
    pub id: u128,
    /// The village it belongs to.
    pub village: VillageId,
    /// Which queue it occupied.
    pub kind: UnitOrderKind,
    /// The unit type to mark researched / level up.
    pub unit: UnitId,
    /// The level to set (Smithy upgrades); `None` for research.
    pub target_level: Option<u8>,
}

/// Persistence for the per-village unit queues (research + Smithy upgrades; due-events, P1).
#[async_trait]
pub trait UnitRepository: Send + Sync {
    /// Atomically settle the village's resources to `settled` (at `now`) and enqueue `order`. The
    /// one-active-order-per-kind rule is enforced by storage; a second active order of the same
    /// kind returns [`RepoError::Duplicate`] (AC6/AC10, P4).
    ///
    /// `settled` was computed from the snapshot read at `settled_from`; the settle applies **only
    /// if the row is still at that snapshot**, otherwise [`RepoError::Conflict`] (see
    /// [`BuildRepository::start_build`]).
    ///
    /// # Errors
    /// [`RepoError`] on conflict or storage failure.
    async fn start_unit_order(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        order: NewUnitOrder,
    ) -> Result<(), RepoError>;

    /// The village's active (pending) unit orders — at most one per [`UnitOrderKind`].
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn active_unit_orders(
        &self,
        village: VillageId,
    ) -> Result<Vec<ActiveUnitOrder>, RepoError>;

    /// Unit types researched in this village (beyond the tier-1 implicit one).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn researched_units(&self, village: VillageId) -> Result<Vec<UnitId>, RepoError>;

    /// Current Smithy upgrade level per unit type (absent = level 0).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn unit_levels(&self, village: VillageId) -> Result<Vec<(UnitId, u8)>, RepoError>;

    /// Atomically claim up to `limit` due unit orders (`pending` → `processing`), nearest-due first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_unit_orders(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueUnitOrder>, RepoError>;

    /// Apply a claimed order (mark researched / set the unit level) and mark it done (idempotent;
    /// AC8/AC12).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_unit_order(&self, due: DueUnitOrder) -> Result<(), RepoError>;
}
