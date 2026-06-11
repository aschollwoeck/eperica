//! Ports — the capabilities the application needs from the outside world.
//!
//! These traits are implemented by the infrastructure layer (databases, password hashing, …). Keeping
//! them here lets use-cases be written and tested against fakes, with no I/O dependency.

use async_trait::async_trait;
use eperica_domain::{
    BuildTarget, BuildingKind, Coordinate, EventKind, MovementKind, PlayerId, QueueLane,
    ResourceAmounts, StartingVillage, Timestamp, TradeKind, Tribe, UnitCounts, UnitId, Village,
    VillageId,
};

/// A village's public presence on the map: its tile and its owner's name (GDD §7.3 — layout and
/// ownership are public; troops/resources are not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VillageMarker {
    /// The tile the village occupies.
    pub coordinate: Coordinate,
    /// The owner's login name.
    pub owner_name: String,
}

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

    /// One village by id (with its fields and buildings) — used by system processors that only
    /// hold a village id (005 starvation checks).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn village_by_id(&self, village: VillageId) -> Result<Option<Village>, RepoError>;

    /// A village's stored resource amounts and the time they were last settled (Unix-ms UTC).
    /// Resources accrue from this snapshot on read (P1); there is no background job.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn stored_resources(
        &self,
        village: VillageId,
    ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError>;

    /// The village's garrison — standing troops per unit type (005; empty before any training).
    /// Part of the economy read path: the garrison's upkeep feeds net crop (AC6).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn garrison(&self, village: VillageId) -> Result<UnitCounts, RepoError>;

    /// Public markers for any villages occupying the given tiles — for the map view (006 AC7).
    /// `coords` should already be canonical (in-bounds) coordinates.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn villages_at(&self, coords: &[Coordinate]) -> Result<Vec<VillageMarker>, RepoError>;

    /// The village occupying `coord` in this world, if any (007 — resolving a movement target).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn village_at(&self, coord: Coordinate) -> Result<Option<Village>, RepoError>;
}

/// An in-flight movement, for the owner's view (007 AC7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovementView {
    /// What it does on arrival.
    pub kind: MovementKind,
    /// Where the troops are heading.
    pub destination: Coordinate,
    /// When they arrive (Unix-ms UTC).
    pub arrive_at: Timestamp,
    /// The composition.
    pub troops: UnitCounts,
}

/// A stationed reinforcement group — the same shape serves "stationed here" (counterparty = the
/// helper) and "my troops abroad" (counterparty = the host), 007 AC7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationedGroup {
    /// Where the troops are stationed.
    pub host_village: VillageId,
    /// The owner's home village (the troops belong here).
    pub home_village: VillageId,
    /// The counterparty village's tile (the home tile when viewed by the host; the host tile when
    /// viewed by the owner).
    pub other_coord: Coordinate,
    /// The counterparty owner's login name.
    pub other_owner: String,
    /// The stationed composition.
    pub troops: UnitCounts,
}

/// A claimed, due movement ready to apply (007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueMovement {
    /// Movement identity.
    pub id: u128,
    /// What to do on arrival.
    pub kind: MovementKind,
    /// The owner's home village.
    pub home_village: VillageId,
    /// The village the troops are delivered to (the target for reinforce, home for return).
    pub deliver_village: VillageId,
    /// The composition.
    pub troops: UnitCounts,
}

/// Persistence for troop movements and stationed reinforcements (due-events, P1; 007).
#[async_trait]
pub trait MovementRepository: Send + Sync {
    /// Atomically debit `troops` from the `home` garrison (guarded: each count must be available)
    /// and create a reinforcement movement to `deliver` arriving at `arrive_at`. The destination
    /// village id is fixed here, so a later ownership change of the tile cannot redirect troops in
    /// flight (P4).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the garrison no longer covers a requested count; [`RepoError`]
    /// otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn start_reinforcement(
        &self,
        home: VillageId,
        deliver: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError>;

    /// Atomically remove the reinforcement group stationed at `host` for the `home` village and
    /// create a **return** movement home arriving at `arrive_at`; returns the moved composition.
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if no group is stationed there (a race); [`RepoError`] otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn start_return(
        &self,
        host: VillageId,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
    ) -> Result<UnitCounts, RepoError>;

    /// The owner's in-flight movements (home village = the owner's).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn active_movements(&self, owner: PlayerId) -> Result<Vec<MovementView>, RepoError>;

    /// Reinforcement groups stationed **at** `village` (counterparty = each helper's home).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn reinforcements_at(&self, village: VillageId)
    -> Result<Vec<StationedGroup>, RepoError>;

    /// The owner's reinforcement groups stationed abroad (counterparty = each host).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn reinforcements_of(&self, owner: PlayerId) -> Result<Vec<StationedGroup>, RepoError>;

    /// Atomically claim movements whose arrival is due (`in_transit → processing`), nearest first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_movements(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueMovement>, RepoError>;

    /// Apply a claimed arrival in **one** transaction — station the troops (reinforce) or rejoin
    /// the garrison (return) and mark the movement done; exactly-once with the orphan requeue
    /// (AC4/AC5).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_movement(&self, due: &DueMovement) -> Result<(), RepoError>;
}

/// An in-flight shipment, for the owner's view (008 AC6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeView {
    /// What this leg does on arrival.
    pub kind: TradeKind,
    /// Where the merchants are heading.
    pub destination: Coordinate,
    /// When they arrive (Unix-ms UTC).
    pub arrive_at: Timestamp,
    /// The carried bundle (all zero on a return leg).
    pub bundle: ResourceAmounts,
    /// Merchants committed to this leg.
    pub merchants: u32,
}

/// A claimed, due trade leg ready to apply (008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueTrade {
    /// Trade-leg identity.
    pub id: u128,
    /// What to do on arrival.
    pub kind: TradeKind,
    /// The sender; the merchants belong to this player's home village.
    pub owner: PlayerId,
    /// The sender's village (merchants belong here; the return leg is delivered here).
    pub home_village: VillageId,
    /// The village credited on a deliver leg.
    pub target_village: VillageId,
    /// This leg's origin tile.
    pub origin: Coordinate,
    /// This leg's destination tile.
    pub dest: Coordinate,
    /// When this leg arrives (Unix-ms UTC) — the return leg departs at this instant (P2).
    pub arrive_at: Timestamp,
    /// The carried bundle (zero on a return leg).
    pub bundle: ResourceAmounts,
    /// Merchants committed to the trade.
    pub merchants: u32,
}

/// Persistence for marketplace trade (due-events, P1; 008). Merchants are not entities: a sender's
/// free count is `merchantsFor(level) − committed_merchants` computed on read.
#[async_trait]
pub trait TradeRepository: Send + Sync {
    /// Merchants the sender currently has committed to in-flight shipments (in_transit + processing
    /// legs — counting `processing` avoids a free-count dip between a deliver's claim and its return).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn committed_merchants(&self, home: VillageId) -> Result<u32, RepoError>;

    /// Atomically debit the shipment from the sender (optimistic settle: `settled` are the sender's
    /// post-debit amounts computed from the `settled_from` snapshot; applies only if the row is
    /// still at that snapshot) and create the `deliver` leg arriving at `arrive_at`. The target
    /// village id is fixed here, so a later ownership change of the tile cannot redirect resources
    /// in flight (P4).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the sender's resources moved since the snapshot; [`RepoError`]
    /// otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn start_trade(
        &self,
        home: VillageId,
        target: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        arrive_at: Timestamp,
        bundle: ResourceAmounts,
        merchants: u32,
    ) -> Result<(), RepoError>;

    /// The owner's in-flight shipments (home village = the owner's), for the village panel.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn active_trades(&self, owner: PlayerId) -> Result<Vec<TradeView>, RepoError>;

    /// Atomically claim trade legs whose arrival is due (`in_transit → processing`), nearest first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_trades(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueTrade>, RepoError>;

    /// Apply a due **deliver** in **one** transaction — credit the target with `target_settled`
    /// (the capped delivery, computed from the `target_from` snapshot and written with the
    /// `credit_clock` as the new settle clock — never earlier than `target_from`), mark the deliver
    /// leg done, and insert the empty `return` leg departing at the true arrival (`due.arrive_at`)
    /// and arriving at `return_arrive`. Exactly-once with the orphan requeue (AC4).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the target's resources moved since the snapshot (nothing applied;
    /// caller re-settles and retries); [`RepoError`] otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn deliver_and_schedule_return(
        &self,
        due: &DueTrade,
        target_settled: ResourceAmounts,
        target_from: Timestamp,
        credit_clock: Timestamp,
        return_arrive: Timestamp,
    ) -> Result<(), RepoError>;

    /// Mark a due **return** leg done (frees its merchants). Exactly-once via the status flip (AC5).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn complete_trade(&self, id: u128) -> Result<(), RepoError>;

    /// Hand a claimed leg back to `in_transit` (`processing → in_transit`) so the next tick retries
    /// it — used when a deliver loses the optimistic credit repeatedly, to avoid stranding the leg
    /// (and its committed merchants) until a restart.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn release_trade(&self, id: u128) -> Result<(), RepoError>;
}

/// A claimed, due attack/raid movement ready to resolve (009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueAttack {
    /// The attack movement's identity (also seeds the battle's luck).
    pub id: u128,
    /// `Attack` or `Raid`.
    pub kind: MovementKind,
    /// The attacker.
    pub owner: PlayerId,
    /// The attacker's home village (survivors return here).
    pub home_village: VillageId,
    /// The village under attack.
    pub target_village: VillageId,
    /// The attacker's tile.
    pub origin: Coordinate,
    /// The target's tile.
    pub dest: Coordinate,
    /// When the attack arrives (the resolution instant).
    pub arrive_at: Timestamp,
    /// The attacking composition.
    pub troops: UnitCounts,
}

/// A battle report to persist, visible to both parties (009 AC7).
#[derive(Debug, Clone, PartialEq)]
pub struct NewBattleReport {
    /// `Attack` or `Raid`.
    pub kind: MovementKind,
    pub attacker_player: PlayerId,
    pub attacker_village: VillageId,
    pub defender_player: PlayerId,
    pub defender_village: VillageId,
    pub attacker_won: bool,
    /// The luck factor that applied (`[1−L, 1+L]`).
    pub luck: f64,
    /// The morale factor that applied (`≤ 1`).
    pub morale: f64,
    pub wall_before: u8,
    pub wall_after: u8,
    /// Each side's forces (sent / defending) and losses, as unit→count maps.
    pub attacker_forces: UnitCounts,
    pub attacker_losses: UnitCounts,
    pub defender_forces: UnitCounts,
    pub defender_losses: UnitCounts,
}

/// The single-transaction application of a resolved battle (009 AC6/AC7).
#[derive(Debug, Clone, PartialEq)]
pub struct BattleApply {
    /// The attack movement to mark `done`.
    pub movement_id: u128,
    /// The attacker (for the survivor return movement).
    pub owner: PlayerId,
    /// The attacker's home village.
    pub attacker_home: VillageId,
    /// The attacker's tile (the return's destination).
    pub attacker_origin: Coordinate,
    /// The target village.
    pub target: VillageId,
    /// The target's tile (the return's origin).
    pub target_coord: Coordinate,
    /// Losses to subtract from the target garrison.
    pub defender_losses: UnitCounts,
    /// Losses to subtract from each reinforcement group (keyed by the group's home village).
    pub reinforcement_losses: Vec<(VillageId, UnitCounts)>,
    /// The attacker's surviving troops (sent home as a `return` movement; empty ⇒ no return).
    pub survivors: UnitCounts,
    /// The resolution instant (the survivor return departs then).
    pub battle_at: Timestamp,
    /// When the survivor return arrives home.
    pub return_arrive: Timestamp,
    /// The report to persist.
    pub report: NewBattleReport,
}

/// A persisted battle report for the inbox/detail view (009 AC8).
#[derive(Debug, Clone, PartialEq)]
pub struct BattleReportView {
    pub id: u128,
    pub occurred_at: Timestamp,
    pub kind: MovementKind,
    pub attacker_name: String,
    pub attacker_coord: Coordinate,
    pub defender_name: String,
    pub defender_coord: Coordinate,
    pub attacker_player: PlayerId,
    pub defender_player: PlayerId,
    pub attacker_won: bool,
    pub luck: f64,
    pub morale: f64,
    pub wall_before: u8,
    pub wall_after: u8,
    pub attacker_forces: UnitCounts,
    pub attacker_losses: UnitCounts,
    pub defender_forces: UnitCounts,
    pub defender_losses: UnitCounts,
}

/// Persistence for combat (009): launch attacks, claim due battles, apply resolutions, read reports.
#[async_trait]
pub trait CombatRepository: Send + Sync {
    /// Atomically debit `troops` from the `home` garrison (guarded) and create an attack/raid
    /// movement of `kind` to `deliver` arriving at `arrive_at`. The target id is fixed here (P4).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the garrison no longer covers a requested count; [`RepoError`]
    /// otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn start_attack(
        &self,
        home: VillageId,
        deliver: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        kind: MovementKind,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError>;

    /// Atomically claim attack/raid movements whose arrival is due (`in_transit → processing`).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_attacks(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueAttack>, RepoError>;

    /// Apply a resolved battle in **one** transaction — subtract the defender's garrison and
    /// reinforcement losses, insert the report, schedule the survivor return (if any), and mark the
    /// attack movement `done`. Exactly-once with the orphan requeue (AC6/AC7).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_battle(&self, apply: BattleApply) -> Result<(), RepoError>;

    /// The player's battle reports (as attacker or defender), newest first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn reports_for(
        &self,
        player: PlayerId,
        limit: i64,
    ) -> Result<Vec<BattleReportView>, RepoError>;

    /// One battle report, only if `player` is a party to it (P4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn report(
        &self,
        id: u128,
        player: PlayerId,
    ) -> Result<Option<BattleReportView>, RepoError>;
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

/// A new training batch to enqueue (005).
#[derive(Debug, Clone)]
pub struct NewTrainingOrder {
    /// The troop building whose queue this batch occupies.
    pub building: BuildingKind,
    /// The unit type being trained.
    pub unit: UnitId,
    /// How many units the batch trains.
    pub count: u32,
    /// Seconds per unit (already building- and speed-scaled).
    pub per_unit_secs: i64,
}

/// A village's currently-running training batch (one per troop building at most).
#[derive(Debug, Clone)]
pub struct ActiveTraining {
    /// The troop building training it.
    pub building: BuildingKind,
    /// The unit type being trained.
    pub unit: UnitId,
    /// Batch size.
    pub count_total: u32,
    /// Units already delivered to the garrison.
    pub count_done: u32,
    /// Seconds per unit.
    pub per_unit_secs: i64,
    /// When the next unit completes (Unix-ms UTC).
    pub next_complete_at: Timestamp,
}

/// A claimed training batch with at least one unit due.
#[derive(Debug, Clone)]
pub struct DueTraining {
    /// Order identity.
    pub id: u128,
    /// The village it belongs to.
    pub village: VillageId,
    /// The unit type being trained.
    pub unit: UnitId,
    /// Batch size.
    pub count_total: u32,
    /// Units already delivered.
    pub count_done: u32,
    /// Seconds per unit.
    pub per_unit_secs: i64,
    /// When the batch started (Unix-ms UTC); completions fall at `started_at + i × per_unit`.
    pub started_at: Timestamp,
}

/// Persistence for training batches and the garrison (due-events, P1; 005).
#[async_trait]
pub trait TrainingRepository: Send + Sync {
    /// Atomically settle the village's resources to `settled` (computed from the snapshot read at
    /// `settled_from`; see [`BuildRepository::start_build`]) and enqueue the batch. The
    /// one-batch-per-building rule is enforced by storage; a busy building returns
    /// [`RepoError::Duplicate`] (AC2, P4).
    ///
    /// # Errors
    /// [`RepoError`] on conflict or storage failure.
    async fn start_training(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        order: NewTrainingOrder,
    ) -> Result<(), RepoError>;

    /// The village's running batches — at most one per troop building.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn active_training(&self, village: VillageId) -> Result<Vec<ActiveTraining>, RepoError>;

    /// Atomically claim batches with a completion due (`active → processing`), nearest first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_training(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueTraining>, RepoError>;

    /// Deliver `completed` finished units to the garrison, settle the village's resources to
    /// `settled` as of `settle_to` (computed piecewise by the caller so upkeep starts at each
    /// unit's own completion instant — spec Decision "troops in training do not eat"), and
    /// advance the batch — all in **one** transaction, so a crash never loses or duplicates a
    /// unit (AC5/AC6). The settle is snapshot-guarded against `settled_from` (see
    /// [`BuildRepository::start_build`]). Re-marks the batch `active` (or `done` when finished).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] when the snapshot moved (nothing applied; release and retry);
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_training(
        &self,
        due: &DueTraining,
        completed: u32,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        settle_to: Timestamp,
    ) -> Result<(), RepoError>;

    /// Return a claimed batch to `active` unchanged (a conflicting settle or a not-yet-due claim);
    /// it is re-claimed on a later tick with a fresh snapshot.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn release_training(&self, due: &DueTraining) -> Result<(), RepoError>;
}

/// Persistence for per-village crop-depletion checks (005 AC7; at most one pending per village).
#[async_trait]
pub trait StarvationRepository: Send + Sync {
    /// Schedule (or move) the village's depletion check to `due_at` and mark it pending — an
    /// upsert, so re-syncing at every mutation point keeps exactly one check per village.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn schedule_starvation_check(
        &self,
        village: VillageId,
        due_at: Timestamp,
    ) -> Result<(), RepoError>;

    /// Remove the village's check (net crop is non-negative or there is no garrison, AC8).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn cancel_starvation_check(&self, village: VillageId) -> Result<(), RepoError>;

    /// Atomically claim due checks (`pending → processing`); returns the affected villages.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_starvation(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<VillageId>, RepoError>;

    /// Apply a cull in **one** transaction: snapshot-guarded resource settle (see
    /// [`BuildRepository::start_build`]), replace the garrison with `survivors`, and mark the
    /// claimed check done — so starvation happens exactly once (AC7).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] when the snapshot moved — nothing is applied; the caller re-pends
    /// the check (`resolve_starvation_check(Some(now))`) so the next tick re-validates from a
    /// fresh snapshot. [`RepoError::Backend`] on storage failure.
    async fn apply_starvation(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        survivors: &UnitCounts,
    ) -> Result<(), RepoError>;

    /// Re-validate outcome: the claimed check is not needed now — reschedule it at the new
    /// depletion time (`Some`) or mark it done (`None`).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn resolve_starvation_check(
        &self,
        village: VillageId,
        reschedule_at: Option<Timestamp>,
    ) -> Result<(), RepoError>;
}
