//! Ports — the capabilities the application needs from the outside world.
//!
//! These traits are implemented by the infrastructure layer (databases, password hashing, …). Keeping
//! them here lets use-cases be written and tested against fakes, with no I/O dependency.

use async_trait::async_trait;
use eperica_domain::{
    AchievementDef, AchievementId, AllianceId, AllianceRole, ArtifactDef, BuildTarget,
    BuildingKind, Coordinate, DiplomacyStance, DiplomacyStatus, EconomyRules, EventKind,
    MedalCategory, MovementKind, NotificationKind, OasisBonus, OasisRules, PlayerId,
    PlayerProgress, Quadrant, QuestDef, QuestId, QuestProgress, QueueLane, ReportReason,
    ResourceAmounts, RightSet, SanctionKind, ScoutTarget, StartingVillage, Timestamp, TradeKind,
    Tribe, UnitCounts, UnitId, UnitSpec, Village, VillageId,
};
use std::collections::HashSet;

/// A village's public presence on the map: its tile and its owner's name (GDD §7.3 — layout and
/// ownership are public; troops/resources are not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VillageMarker {
    /// The tile the village occupies.
    pub coordinate: Coordinate,
    /// The owner's login name.
    pub owner_name: String,
    /// The owner's alliance **tag**, if they are in one (public, §7.3; 015 AC11).
    pub alliance_tag: Option<String>,
    /// The owner's last activity (Unix-ms) — the map view derives the **inactive/farmable** flag from
    /// it via [`eperica_domain::is_inactive`] (019 AC6).
    pub owner_last_activity: Timestamp,
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
    /// Whether the account has been **abandoned** by the inactivity sweep (019) — retired: cannot log
    /// in, hidden from rankings, but the row is retained for historical referential integrity.
    pub abandoned: bool,
    /// Whether the account holds the elevated **Moderator** role (022) — additive to Player.
    pub is_moderator: bool,
    /// When the account was permanently **banned** (022), or `None`. A ban always blocks.
    pub banned_at: Option<Timestamp>,
    /// The instant a temporary **suspension** lifts (022), or `None`. Blocks while `now` is before it.
    pub suspended_until: Option<Timestamp>,
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

    /// A player's beginner's-protection expiry (019), or `None` if never granted / already ended. The
    /// pure [`eperica_domain::is_protected`] turns this into the protected/unprotected decision.
    ///
    /// Defaults to "never protected" so non-account fakes need not implement it; the real adapter
    /// overrides it.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn protection_of(&self, _player: PlayerId) -> Result<Option<Timestamp>, RepoError> {
        Ok(None)
    }

    /// End a player's beginner's protection now (019 AC3/AC4) — sets `protected_until = now`, but only
    /// while a window is still active (idempotent; never extends or re-arms protection). Defaults to a
    /// no-op; the real adapter overrides it.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn end_protection(&self, _player: PlayerId, _now: Timestamp) -> Result<(), RepoError> {
        Ok(())
    }

    /// Refresh a player's `last_activity` (019 AC5), **throttled**: a no-op unless the stored value is
    /// already older than the implementation's freshness interval, so it is a cheap conditional write.
    /// Defaults to a no-op; the real adapter overrides it.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn touch_activity(&self, _player: PlayerId, _now: Timestamp) -> Result<(), RepoError> {
        Ok(())
    }

    /// Set (or clear) a player's profile bio (025). Defaults to a no-op.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn set_bio(&self, _player: PlayerId, _bio: &str) -> Result<(), RepoError> {
        Ok(())
    }

    /// A player's public profile (name + bio + last activity for presence, 025), or `None` if unknown.
    /// Defaults to `None`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn profile_of(&self, _player: PlayerId) -> Result<Option<ProfileView>, RepoError> {
        Ok(None)
    }

    /// Players whose username is a case-insensitive **prefix** of `query` (028 AC1), excluding abandoned +
    /// NPC accounts, ordered by name, capped at `limit`. Defaults to empty.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn search_players(&self, _query: &str, _limit: i64) -> Result<Vec<PlayerHit>, RepoError> {
        Ok(Vec::new())
    }
}

/// A public player search hit (028 AC1) — id + display name only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerHit {
    pub player: PlayerId,
    pub name: String,
}

/// A public alliance search hit (028 AC2) — id + name + tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllianceHit {
    pub alliance: AllianceId,
    pub name: String,
    pub tag: String,
}

/// A player's public profile (025): identity, bio, and the activity instant presence is derived from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileView {
    /// The player.
    pub player: PlayerId,
    /// Display name.
    pub name: String,
    /// Free-text bio (empty if unset).
    pub bio: String,
    /// Last activity (Unix-ms UTC) — the input to the presence rule.
    pub last_activity: Timestamp,
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
    /// The home village's tribe (selects the roster for combat defence, 009).
    pub home_tribe: Option<Tribe>,
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
    /// Loot this movement carries home (011) — non-zero only on a `return` from a raid/attack.
    pub loot: ResourceAmounts,
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
    /// `credit` (011) is the snapshot-guarded loot credit for a `return` that carried loot — applied
    /// to the home village's resources in the same transaction as the garrison rejoin.
    async fn apply_movement(
        &self,
        due: &DueMovement,
        credit: Option<ResourceWrite>,
    ) -> Result<(), RepoError>;
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
    /// What the attached scouts spy on (010); `None` when the attack carries no scouting intent.
    pub scout_target: Option<ScoutTarget>,
    /// The building the attached catapults aim at (011); `None` = no catapults / seeded-random target.
    pub catapult_target: Option<BuildingKind>,
}

/// A building a battle razed with catapults (011) — its kind and the levels before/after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RazedBuilding {
    pub kind: BuildingKind,
    pub before: u8,
    pub after: u8,
}

/// A snapshot-guarded resource write: the settled-and-adjusted amounts, the snapshot they were
/// computed from, and the new settle clock — the 008-deliver pattern reused for the loot debit (011
/// target) and the loot credit (011 attacker return).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceWrite {
    /// The amounts to write (already settled to `clock` and loot-adjusted).
    pub after: ResourceAmounts,
    /// The snapshot the amounts were settled from (the write is guarded on it, P2/P4).
    pub settled_from: Timestamp,
    /// The new `updated_at` clock (never earlier than `settled_from`).
    pub clock: Timestamp,
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
    /// Resources the attacker looted (011); all-zero when nothing was taken.
    pub loot: ResourceAmounts,
    /// The building catapults razed (011), or `None`.
    pub razed: Option<RazedBuilding>,
    /// The target's loyalty **before** the administrator strike (014 AC10); `None` for a battle that
    /// carried no administrator / did not win (no loyalty change).
    pub loyalty_before: Option<i64>,
    /// The target's loyalty **after** the strike; `None` as above.
    pub loyalty_after: Option<i64>,
    /// Whether the village **changed hands** (014 AC4).
    pub conquered: bool,
}

/// A surviving third-party reinforcement group sent **home** when its host village is conquered (014
/// AC7) — the 007 return, computed by the application (it holds the map/speed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReinforcementReturn {
    /// The group's home village (the return's destination; also the reinforcements key to clear).
    pub home_village: VillageId,
    /// The group's owner.
    pub owner: PlayerId,
    /// The home village's tile (the return's destination).
    pub home_coord: Coordinate,
    /// The surviving composition to send back.
    pub troops: UnitCounts,
    /// When the returning troops arrive home.
    pub arrive_at: Timestamp,
}

/// The ownership transfer of a conquered village (014 AC4/AC7), applied in the battle transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConquestTransfer {
    /// The conqueror (the village's new owner).
    pub new_owner: PlayerId,
    /// The losing previous owner.
    pub loser: PlayerId,
    /// Loyalty the conquered village resets to (so it cannot be re-taken immediately).
    pub post_conquest_loyalty: i64,
    /// The **loser's** culture settled to the battle instant at the OLD rate (the village still counted
    /// toward their rate) — written before ownership moves (013 AC1/P2).
    pub loser_culture_value: i64,
    /// The **conqueror's** culture settled to the battle instant at the OLD rate (before the new village
    /// joins their rate).
    pub gainer_culture_value: i64,
    /// Surviving third-party reinforcements to send home (usually empty — a winning attack wipes them).
    pub reinforcement_returns: Vec<ReinforcementReturn>,
}

/// The post-battle loyalty/conquest result (014), attached to a **won** attack that carried
/// administrators; `None` for an ordinary battle (no loyalty change).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoyaltyApply {
    /// Loyalty dropped but the village **held** — persist `new_loyalty`, anchored at the battle instant.
    Reduced { new_loyalty: i64 },
    /// The village was **conquered** — transfer ownership in the battle transaction.
    Conquered(ConquestTransfer),
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
    /// Whether the attached scouts were **detected** (≥1 died to counter-espionage) — sets the
    /// defender battle report's `scouted` flag (010 AC8); `false` when no scouts rode along.
    pub scouted: bool,
    /// What the attached scouts spied on (mirrors `scouted`), for the defender's report.
    pub scout_target: Option<ScoutTarget>,
    /// The scouter-facing intel report to persist alongside the battle (010), if scouts rode along.
    pub scout_report: Option<NewScoutReport>,
    /// Resources the attacker looted (011) — attached to the survivor `return` and the report.
    pub loot: ResourceAmounts,
    /// The target's settled, looted-down resources to write (011), snapshot-guarded; `None` = no loot.
    pub target_debit: Option<ResourceWrite>,
    /// The building catapults razed (011) — decremented on the target; `None` = none.
    pub razed: Option<RazedBuilding>,
    /// The post-battle loyalty result (014): `Reduced` writes the new loyalty; `Conquered` transfers
    /// ownership in this transaction. `None` for an ordinary battle (no administrators / a loss).
    pub loyalty: Option<LoyaltyApply>,
    /// The battle's **attack points** (016 AC4): the valued defender troops the attacker killed,
    /// credited to the attacker. Persisted as a fact on the report (summed on read).
    pub attack_points: i64,
    /// Per-defending-player contributions (016 AC3/AC4): one per defending player — the village owner
    /// (`is_owner = true`) and each reinforcer — carrying their forces, losses, contributed defensive
    /// value, and split defense points. Persisted as `battle_defenders` rows in this transaction;
    /// empty for a battle with no recorded player defenders (e.g. PvE, or a legacy test).
    pub defender_contributions: Vec<DefenderContribution>,
    /// An artifact captured by this winning attack (020 AC4/AC5): its id moves to the attacking
    /// village in the battle transaction. `None` when nothing is captured.
    pub artifact_capture: Option<ArtifactCapture>,
    /// A Wonder plan captured by this winning attack (021 AC2): its id moves to the attacking village
    /// in the battle transaction (the same mechanic as an artifact). `None` when nothing is captured.
    pub plan_capture: Option<PlanCapture>,
}

/// An artifact transferred by a winning attack (020): the artifact id and the attacking village it
/// moves to.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactCapture {
    /// The captured artifact's id.
    pub artifact_id: String,
    /// The village it was taken **from** (the target) — the transfer is guarded on this so a
    /// concurrent capture of the same artifact affects zero rows (exactly-once, P5).
    pub from_village: VillageId,
    /// The attacking village it moves to.
    pub to_village: VillageId,
}

/// A Wonder plan transferred by a winning attack (021): the plan id and the attacking village it
/// moves to. Mirrors [`ArtifactCapture`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCapture {
    /// The captured plan's id.
    pub plan_id: String,
    /// The village it was taken **from** (the target) — the transfer is guarded on this so a
    /// concurrent capture affects zero rows (exactly-once, P5).
    pub from_village: VillageId,
    /// The attacking village it moves to.
    pub to_village: VillageId,
}

/// One defending player's share of a battle (016 AC3/AC4): the troops they contributed, their
/// losses, the defensive value they brought (the [`apportion`](eperica_domain::apportion) weight),
/// and their resulting defense points. The target's garrison owner is one of these (`is_owner =
/// true`) alongside each reinforcing player; a reinforcer reads their own row as their report (§9.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefenderContribution {
    /// The defending player (the village owner, or a reinforcer).
    pub player: PlayerId,
    /// The home village of this defender's troops (the owner's target, or the reinforcer's home).
    pub village: VillageId,
    /// Whether this is the target village's garrison owner (vs. a reinforcing player).
    pub is_owner: bool,
    /// The troops this player had defending in the battle.
    pub forces: UnitCounts,
    /// This player's losses.
    pub losses: UnitCounts,
    /// The defensive value this player contributed — the weight their defense-point share is by.
    pub defense_value: i64,
    /// This player's split of the battle's defense-point total.
    pub defense_points: i64,
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
    /// The defending player, or `None` when the defender is a village-less **oasis** (wild animals,
    /// 012) — only the attacker is a party to such a report.
    pub defender_player: Option<PlayerId>,
    pub attacker_won: bool,
    pub luck: f64,
    pub morale: f64,
    pub wall_before: u8,
    pub wall_after: u8,
    pub attacker_forces: UnitCounts,
    pub attacker_losses: UnitCounts,
    pub defender_forces: UnitCounts,
    pub defender_losses: UnitCounts,
    /// Whether scouts rode along and were **detected** (010 AC8) — the defender's report flags it.
    pub scouted: bool,
    /// What those scouts spied on, when `scouted`.
    pub scout_target: Option<ScoutTarget>,
    /// Resources the attacker looted (011); all-zero when nothing was taken.
    pub loot: ResourceAmounts,
    /// The building catapults razed (011), or `None`.
    pub razed: Option<RazedBuilding>,
    /// The target's loyalty before → after the administrator strike (014 AC10); `None` for a battle
    /// with no loyalty change.
    pub loyalty_before: Option<i64>,
    pub loyalty_after: Option<i64>,
    /// Whether the village changed hands (014 AC4).
    pub conquered: bool,
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
        scout_target: Option<ScoutTarget>,
        catapult_target: Option<BuildingKind>,
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

/// Intel a successful scout brought home (010 AC9) — what the chosen target type revealed at arrival.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoutIntel {
    /// The target village's stored resources at the resolution instant (computed-on-read, P1).
    Resources(ResourceAmounts),
    /// The target's stationed troops (garrison + reinforcements, merged) and Wall level.
    Defenses { troops: UnitCounts, wall_level: u8 },
}

/// A claimed, due standalone `scout` movement ready to resolve (010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueScout {
    /// The scout movement's identity.
    pub id: u128,
    /// The scouting player.
    pub owner: PlayerId,
    /// The scouter's home village (survivors return here).
    pub home_village: VillageId,
    /// The village being scouted.
    pub target_village: VillageId,
    /// The scouter's tile.
    pub origin: Coordinate,
    /// The target's tile.
    pub dest: Coordinate,
    /// When the scouts arrive (the resolution instant).
    pub arrive_at: Timestamp,
    /// The scouting composition (scouts only).
    pub troops: UnitCounts,
    /// What this mission spies on.
    pub target_type: ScoutTarget,
}

/// A scout intel report to persist (010 AC8/AC11). Visible in full to the scouter; visible redacted
/// to the target only when `detected && standalone`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScoutReport {
    pub scouter_player: PlayerId,
    pub scouter_village: VillageId,
    pub target_player: PlayerId,
    pub target_village: VillageId,
    /// The target's tile (shown to the scouter).
    pub target_coord: Coordinate,
    pub target_type: ScoutTarget,
    /// The scouts sent (scouter-only).
    pub scouts_sent: UnitCounts,
    /// The scouts lost to counter-espionage (also "scouts destroyed" for a notified target).
    pub scouts_lost: UnitCounts,
    /// Whether the defender detected the mission (≥1 scout died).
    pub detected: bool,
    /// Standalone mission (`true`) vs scouts riding an attack (`false`) — gates the target's view.
    pub standalone: bool,
    /// The revealed intel, or `None` when no scout survived to carry it home.
    pub intel: Option<ScoutIntel>,
}

/// The single-transaction application of a resolved **standalone** scout mission (010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutApply {
    /// The scout movement to mark `done`.
    pub movement_id: u128,
    /// The scouter (for the survivor return movement).
    pub owner: PlayerId,
    /// The scouter's home village.
    pub scouter_home: VillageId,
    /// The scouter's tile (the return's destination).
    pub scouter_origin: Coordinate,
    /// The target's tile (the return's origin).
    pub target_coord: Coordinate,
    /// The surviving scouts (sent home as a `return` movement; empty ⇒ no return).
    pub survivors: UnitCounts,
    /// The resolution instant (the survivor return departs then).
    pub scouted_at: Timestamp,
    /// When the survivor return arrives home.
    pub return_arrive: Timestamp,
    /// The intel report to persist.
    pub report: NewScoutReport,
}

/// A persisted scout report for the inbox/detail view (010 AC11). The repository applies redaction:
/// for a target viewer it strips the intel and the scouts-sent, leaving only the notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutReportView {
    pub id: u128,
    pub occurred_at: Timestamp,
    pub scouter_player: PlayerId,
    pub scouter_name: String,
    pub scouter_coord: Coordinate,
    pub target_player: PlayerId,
    pub target_name: String,
    pub target_coord: Coordinate,
    pub target_type: ScoutTarget,
    /// The scouts sent — empty when the viewer is the target (redacted, P4).
    pub scouts_sent: UnitCounts,
    pub scouts_lost: UnitCounts,
    pub detected: bool,
    pub standalone: bool,
    /// The revealed intel — `None` when the viewer is the target, or no scout returned.
    pub intel: Option<ScoutIntel>,
    /// Whether the requesting player is the scouter (drives the template; redaction already applied).
    pub viewer_is_scouter: bool,
}

/// Persistence for scouting (010): launch standalone scouts, claim due missions, apply resolutions,
/// read intel reports. (Scouts riding an attack are handled by [`CombatRepository::apply_battle`].)
#[async_trait]
pub trait ScoutRepository: Send + Sync {
    /// Atomically debit `troops` (scouts) from the `home` garrison (guarded) and create a `scout`
    /// movement to `deliver` arriving at `arrive_at` with the chosen `target`. Target id fixed (P4).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the garrison no longer covers a requested count; [`RepoError`] else.
    #[allow(clippy::too_many_arguments)]
    async fn start_scout(
        &self,
        home: VillageId,
        deliver: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
        target: ScoutTarget,
    ) -> Result<(), RepoError>;

    /// Atomically claim standalone scout movements whose arrival is due (`in_transit → processing`).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_scouts(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueScout>, RepoError>;

    /// Apply a resolved standalone scout in **one** transaction — insert the intel report, schedule
    /// the survivor return (if any), and mark the scout movement `done`. Exactly-once (010 AC10/AC11).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_scout(&self, apply: ScoutApply) -> Result<(), RepoError>;

    /// The player's scout reports — their own missions (full), plus detected-standalone
    /// notifications where they were the target (redacted), newest first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn scout_reports_for(
        &self,
        player: PlayerId,
        limit: i64,
    ) -> Result<Vec<ScoutReportView>, RepoError>;

    /// One scout report, only if `player` may see it (scouter, or a detected-standalone target),
    /// redacted for a target viewer (P4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn scout_report(
        &self,
        id: u128,
        player: PlayerId,
    ) -> Result<Option<ScoutReportView>, RepoError>;
}

/// An oasis's persisted state (012): its owner (`None` ⇒ unoccupied, wild animals defend) and
/// whether a row has been materialised yet (an un-materialised oasis uses the seeded animals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OasisState {
    /// The occupying village, or `None` when the oasis is unoccupied.
    pub owner: Option<VillageId>,
    /// Whether a persisted row exists (the oasis has been fought/occupied at least once).
    pub materialised: bool,
}

/// A claimed, due oasis-attack movement ready to resolve (012). Targets a **tile**, not a village.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueOasisAttack {
    /// The movement's identity (also seeds the battle's luck).
    pub id: u128,
    /// The attacker.
    pub owner: PlayerId,
    /// The attacker's home village (survivors return here).
    pub home_village: VillageId,
    /// The attacker's tile.
    pub origin: Coordinate,
    /// The oasis tile under attack.
    pub oasis: Coordinate,
    /// When the attack arrives (the resolution instant).
    pub arrive_at: Timestamp,
    /// The attacking composition.
    pub troops: UnitCounts,
}

/// What happens to an oasis's ownership at the end of a resolved oasis battle (012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OasisOwnership {
    /// Leave ownership unchanged (cleared without free capacity, or the attacker lost).
    Unchanged,
    /// The attacker's village occupies/takes the oasis (AC4/AC5).
    Occupy(VillageId),
    /// Free the oasis — clear its owner (defenders wiped, attacker had no capacity; AC5).
    Free,
}

/// An oasis battle report to persist (012 AC11), on the 009 `battle_reports` rails. The defending
/// "village" is a tile + a synthetic label; `defender_player`/`defender_village` are set only when
/// the oasis was **occupied** (a player owned it), `None` for a wild-animal defence.
#[derive(Debug, Clone, PartialEq)]
pub struct NewOasisReport {
    pub attacker_player: PlayerId,
    pub attacker_village: VillageId,
    /// The defending owner, when the oasis was occupied (`None` ⇒ wild animals).
    pub defender_player: Option<PlayerId>,
    /// The defending owner's village, when occupied (`None` ⇒ wild animals).
    pub defender_village: Option<VillageId>,
    /// The oasis tile (stands in for the joined defender village's coordinate).
    pub oasis: Coordinate,
    /// The display label for the defender (e.g. `"Oasis"`).
    pub label: String,
    pub attacker_won: bool,
    pub luck: f64,
    pub morale: f64,
    /// Forces sent / defending and the losses each took, as unit→count maps.
    pub attacker_forces: UnitCounts,
    pub attacker_losses: UnitCounts,
    pub defender_forces: UnitCounts,
    pub defender_losses: UnitCounts,
}

/// The single-transaction application of a resolved oasis battle (012 AC3/AC4/AC10/AC11).
#[derive(Debug, Clone, PartialEq)]
pub struct OasisBattleApply {
    /// The oasis-attack movement to mark `done`.
    pub movement_id: u128,
    /// The attacker (for the survivor return movement).
    pub owner: PlayerId,
    /// The attacker's home village.
    pub attacker_home: VillageId,
    /// The attacker's tile (the return's destination).
    pub attacker_origin: Coordinate,
    /// The oasis tile (the return's origin).
    pub oasis: Coordinate,
    /// The oasis's defenders after the battle — the garrison table is replaced with these (empty ⇒
    /// cleared).
    pub defenders_after: UnitCounts,
    /// The ownership outcome to persist.
    pub ownership: OasisOwnership,
    /// The attacker's surviving troops (sent home as a `return` movement; empty ⇒ no return).
    pub survivors: UnitCounts,
    /// The resolution instant (the survivor return departs then).
    pub battle_at: Timestamp,
    /// When the survivor return arrives home.
    pub return_arrive: Timestamp,
    /// The battle report to persist in the same transaction (AC11).
    pub report: NewOasisReport,
    /// When the oasis should next regrow its animals (012 AC9) — `Some` when it ends **unoccupied**
    /// (so its animals top back up over time), `None` when it ends occupied (no regrow).
    pub regrow_at: Option<Timestamp>,
}

/// A claimed, due oasis regrow (012 AC9): a cleared, unoccupied oasis whose animals should top up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueOasisRegrow {
    /// The oasis tile.
    pub oasis: Coordinate,
    /// Its current (partial) animal garrison.
    pub current: UnitCounts,
    /// The `regrow_at` the row currently holds (guards the apply against a concurrent change).
    pub regrow_at: Timestamp,
}

/// Persistence for oases (012): launch oasis attacks, read defenders/ownership/bonus, claim due
/// oasis battles, apply resolutions. The seeded wild-animal fallback is computed here (the world seed
/// lives in the infrastructure map), so the seeded-animal balance is injected by the application.
#[async_trait]
pub trait OasisRepository: Send + Sync {
    /// The oasis's persisted state at `coord`, or `None` if no row has been materialised yet.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn oasis_at(&self, coord: Coordinate) -> Result<Option<OasisState>, RepoError>;

    /// The oasis's **current defenders** at `coord`: the materialised garrison (wild animals while
    /// unoccupied, or the owner's stationed troops while occupied), or — if no row is materialised —
    /// the **seeded** wild animals computed from the world seed + `animals`/`rules` (P6).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn oasis_defenders(
        &self,
        coord: Coordinate,
        animals: &[UnitSpec],
        rules: &OasisRules,
    ) -> Result<UnitCounts, RepoError>;

    /// The village's occupied oases, each with its per-resource production bonus (for the Outpost
    /// capacity check + the bonus read path; the bonus is derived from the seeded map, not stored).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn occupied_oases(
        &self,
        village: VillageId,
    ) -> Result<Vec<(Coordinate, OasisBonus)>, RepoError>;

    /// The summed per-resource bonus of the village's occupied oases (008-style bonus read path;
    /// AC8). Per-resource values saturate at `u8::MAX`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn village_oasis_bonus(&self, village: VillageId) -> Result<OasisBonus, RepoError>;

    /// The **occupied** oases among `coords`, each with its owner's login name — for the map view
    /// (012 AC12). `coords` should already be canonical (in-bounds) coordinates.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn oasis_owners_at(
        &self,
        coords: &[Coordinate],
    ) -> Result<Vec<(Coordinate, String)>, RepoError>;

    /// Atomically debit `troops` from the `home` garrison (guarded) and create an `oasis_attack`
    /// movement to the `oasis` tile (no destination village) arriving at `arrive_at`.
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the garrison no longer covers a requested count; [`RepoError`] else.
    #[allow(clippy::too_many_arguments)]
    async fn start_oasis_attack(
        &self,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        oasis: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError>;

    /// Atomically claim oasis-attack movements whose arrival is due (`in_transit → processing`).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_oasis_attacks(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueOasisAttack>, RepoError>;

    /// Apply a resolved oasis battle in **one** transaction — materialise the oasis row, replace its
    /// garrison with `defenders_after`, set ownership per `ownership`, schedule the survivor return
    /// (if any), and mark the movement `done`. Exactly-once with the orphan requeue (AC10).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_oasis_battle(&self, apply: OasisBattleApply) -> Result<(), RepoError>;

    /// Atomically debit `troops` from the `home` garrison (guarded) and create an `oasis_reinforce`
    /// movement to the `oasis` tile the sender owns (AC7), arriving at `arrive_at`.
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the garrison no longer covers a requested count; [`RepoError`] else.
    #[allow(clippy::too_many_arguments)]
    async fn start_oasis_reinforce(
        &self,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        oasis: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError>;

    /// Atomically claim oasis-reinforce movements whose arrival is due (`in_transit → processing`).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_oasis_reinforcements(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueOasisReinforce>, RepoError>;

    /// Apply a due oasis reinforcement in **one** transaction (AC7): `Station` adds the troops to the
    /// oasis garrison (the sender still owns it); `BounceHome` instead sends them back as a `return`
    /// (the sender lost the oasis in flight). Marks the movement `done`. Exactly-once.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_oasis_reinforce(
        &self,
        due: &DueOasisReinforce,
        outcome: OasisReinforceOutcome,
    ) -> Result<(), RepoError>;

    /// Recall the troops a player has stationed at an oasis they own (AC7): atomically read+delete the
    /// oasis garrison and create a `return` movement home arriving at `arrive_at`; returns the recalled
    /// composition. The oasis stays owned but undefended.
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if nothing is stationed there (a race); [`RepoError`] otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn start_oasis_recall(
        &self,
        oasis: Coordinate,
        home: VillageId,
        owner: PlayerId,
        home_coord: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
    ) -> Result<UnitCounts, RepoError>;

    /// Claim cleared, **unoccupied** oases whose `regrow_at` is due (012 AC9) — nearest first. Does
    /// not mutate (the apply guards on `regrow_at`), so each is re-claimed until applied.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_oasis_regrows(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueOasisRegrow>, RepoError>;

    /// Apply a regrow tick in **one** transaction: set the oasis garrison to `garrison` and the next
    /// `regrow_at` — guarded on the still-unoccupied oasis row holding `prev_regrow_at` (so occupying
    /// the oasis in flight cancels the regrow, and a concurrent tick applies once). `next_regrow_at`
    /// is `None` once the oasis is back to full strength.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_oasis_regrow(
        &self,
        oasis: Coordinate,
        garrison: &UnitCounts,
        prev_regrow_at: Timestamp,
        next_regrow_at: Option<Timestamp>,
    ) -> Result<(), RepoError>;
}

/// A claimed, due oasis-reinforce movement ready to apply (012 AC7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueOasisReinforce {
    /// The movement's identity.
    pub id: u128,
    /// The sender (the troops belong to this player's home village).
    pub owner: PlayerId,
    /// The sender's home village (where the troops came from / bounce back to).
    pub home_village: VillageId,
    /// The sender's tile.
    pub origin: Coordinate,
    /// The destination oasis tile.
    pub oasis: Coordinate,
    /// When the reinforcement arrives.
    pub arrive_at: Timestamp,
    /// The reinforcing composition.
    pub troops: UnitCounts,
}

/// What to do with a due oasis reinforcement once the sender's ownership is re-checked at arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OasisReinforceOutcome {
    /// The sender still owns the oasis — station the troops as its defenders.
    Station,
    /// The sender no longer owns the oasis — send the troops home (a `return` to `home_coord`).
    BounceHome {
        /// The home village's tile (the return's destination).
        home_coord: Coordinate,
        /// When the bounced troops arrive home.
        return_arrive: Timestamp,
    },
}

/// Persistence for the per-player culture-point accumulator (013, §11.1). CP is pooled across a
/// player's villages and **lazy** (002 model): the stored `(value, updated_at)` is settled on read at
/// the **live rate** (derived from the villages' Town Hall levels), and re-anchored whenever that rate
/// changes.
#[async_trait]
pub trait CultureRepository: Send + Sync {
    /// The player's stored culture-point accumulator: `(value, last-settled instant)`. A freshly
    /// registered player has `(0, registrationTime)`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn player_culture(&self, player: PlayerId) -> Result<(i64, Timestamp), RepoError>;

    /// Re-anchor the accumulator: write the settled `value` and set the last-settled instant to `at`
    /// (an upsert). Called at each rate change so the next read accrues at the new rate from `at`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn settle_culture(
        &self,
        player: PlayerId,
        value: i64,
        at: Timestamp,
    ) -> Result<(), RepoError>;

    /// The Town Hall level of **each** of the player's villages (0 where there is no Town Hall) — the
    /// input to the live culture rate. One entry per owned village.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn village_town_hall_levels(&self, player: PlayerId) -> Result<Vec<u8>, RepoError>;
}

/// Persistence for per-village **loyalty** (014, §3.4). Loyalty is **lazy** (002 model): the stored
/// `(value, updated_at)` is **regenerated on read** toward `MAX_LOYALTY` and re-anchored when an
/// administrator strike changes it. (The ownership transfer itself rides the 009 [`CombatRepository`]
/// battle apply — this port covers the loyalty read/anchor.)
#[async_trait]
pub trait ConquestRepository: Send + Sync {
    /// The village's stored loyalty accumulator: `(value, last-settled instant)`. `None` if the
    /// village does not exist.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn village_loyalty(
        &self,
        village: VillageId,
    ) -> Result<Option<(i64, Timestamp)>, RepoError>;

    /// Re-anchor the village's loyalty: write the settled `value` and set the last-settled instant to
    /// `at` (an upsert on the village row). Called when an administrator strike lowers loyalty without
    /// taking the village, so the next read regenerates from `at`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn set_loyalty(
        &self,
        village: VillageId,
        value: i64,
        at: Timestamp,
    ) -> Result<(), RepoError>;
}

/// A player's alliance membership (015): which alliance, their role, and (for leaders) their rights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    /// The alliance the player belongs to.
    pub alliance: AllianceId,
    /// The player's rank.
    pub role: AllianceRole,
    /// The leader's granted rights (empty for a plain member; the founder implicitly holds all).
    pub rights: RightSet,
}

/// One row of an alliance's roster (015 AC8/AC11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterEntry {
    /// The member.
    pub player: PlayerId,
    /// Their login name.
    pub name: String,
    /// Their rank.
    pub role: AllianceRole,
    /// Their granted rights.
    pub rights: RightSet,
}

/// One row of an alliance's forum thread list (027 AC1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    pub id: u128,
    pub title: String,
    /// The thread starter's display name.
    pub author_name: String,
    /// A one-way announcement (locked to replies, 027 AC4).
    pub announcement: bool,
    /// Number of posts in the thread.
    pub post_count: i64,
    /// Most-recent activity (Unix-ms UTC).
    pub last_post_ms: i64,
}

/// A forum thread's header (027) — used to access-check + lock-check before reading/replying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadHead {
    /// The alliance that owns the thread (must equal the viewer's for access, AC5).
    pub alliance: AllianceId,
    pub title: String,
    /// Locked to replies when true (an announcement, AC3/AC4).
    pub announcement: bool,
}

/// One post in a forum thread (027 AC1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumPost {
    /// The author's display name.
    pub author_name: String,
    pub body: String,
    /// When posted (Unix-ms UTC).
    pub created_ms: i64,
}

/// A pending invitation as seen by the **invited player** (015 AC3/AC11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInvite {
    /// The inviting alliance.
    pub alliance: AllianceId,
    /// Its name.
    pub alliance_name: String,
    /// Its tag.
    pub alliance_tag: String,
}

/// A pending invitation as seen from **inside the alliance** (the management view, 015 AC11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingInvite {
    /// The invited player.
    pub invitee: PlayerId,
    /// Their login name.
    pub invitee_name: String,
}

/// One diplomacy relationship as seen from an alliance (015 AC7/AC11): the other alliance, the stance,
/// its status, and — for a confederation proposal — which alliance proposed it (only the *other* side
/// may accept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiplomacyEntry {
    /// The counterpart alliance.
    pub other: AllianceId,
    /// Its name.
    pub other_name: String,
    /// Its tag.
    pub other_tag: String,
    /// The stance (war / confederation).
    pub stance: DiplomacyStance,
    /// Whether it is active or a pending confederation proposal.
    pub status: DiplomacyStatus,
    /// Who proposed a (still-pending) confederation; `None` for war / once active.
    pub proposed_by: Option<AllianceId>,
}

/// A village belonging to an allied player (a fellow member or a confederate), for the shared village
/// list (015 AC8). Coordinates + names only — never troops or resources (P4/§7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlliedVillage {
    /// The owning (allied) player.
    pub player: PlayerId,
    /// Their login name.
    pub owner_name: String,
    /// The village identity.
    pub village: VillageId,
    /// Its tile.
    pub coordinate: Coordinate,
}

/// An incoming hostile movement against an allied village (015 AC9): the target and arrival time only —
/// never the attacker's composition (hidden, P4/§7.3, until scouted/resolved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingAttack {
    /// The village under threat.
    pub target: VillageId,
    /// Its tile.
    pub coordinate: Coordinate,
    /// When the attack lands.
    pub arrive_at: Timestamp,
}

/// Persistence for alliances & membership (015). Identity (alliance id, the founder's membership) is
/// assigned by the repository.
#[async_trait]
pub trait AllianceRepository: Send + Sync {
    /// The player's **highest** Embassy level across their villages (0 if none) — the join/found gate
    /// (AC1). Computed on read from the building rows (P1).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn max_embassy_level(&self, player: PlayerId) -> Result<u8, RepoError>;

    /// The player's membership, or `None` if they are in no alliance.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn alliance_of(&self, player: PlayerId) -> Result<Option<Membership>, RepoError>;

    /// How many members the alliance holds (the cap input, AC4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn member_count(&self, alliance: AllianceId) -> Result<u32, RepoError>;

    /// The alliance's `(name, tag)`, or `None` if it does not exist.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn alliance_summary(
        &self,
        alliance: AllianceId,
    ) -> Result<Option<(String, String)>, RepoError>;

    /// The alliance's roster, ordered by rank then name (AC8/AC11).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn roster(&self, alliance: AllianceId) -> Result<Vec<RosterEntry>, RepoError>;

    /// Create an alliance with `name`/`tag`, inserting `founder` as its sole **Founder** member, in one
    /// transaction; returns the new alliance's id (AC2). The repository assigns the id.
    ///
    /// # Errors
    /// [`RepoError::Duplicate`] if the name or tag is taken (or the founder is somehow already a
    /// member); [`RepoError::Backend`] otherwise.
    async fn create_alliance(
        &self,
        name: &str,
        tag: &str,
        founder: PlayerId,
    ) -> Result<AllianceId, RepoError>;

    /// Insert a pending invitation `(alliance, invitee)`.
    ///
    /// # Errors
    /// [`RepoError::Duplicate`] if the invitee is already invited to this alliance; [`RepoError`]
    /// otherwise.
    async fn insert_invite(&self, alliance: AllianceId, invitee: PlayerId)
    -> Result<(), RepoError>;

    /// Delete a pending invitation `(alliance, invitee)` (decline / revoke / consumed-on-accept). A
    /// no-op if it does not exist.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn delete_invite(&self, alliance: AllianceId, invitee: PlayerId)
    -> Result<(), RepoError>;

    /// Whether `(alliance, invitee)` has a pending invitation (the accept guard).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn has_invite(&self, alliance: AllianceId, invitee: PlayerId) -> Result<bool, RepoError>;

    /// Pending invitations addressed to `player` (their inbox view, AC3/AC11).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn pending_invites_for(&self, player: PlayerId) -> Result<Vec<PendingInvite>, RepoError>;

    /// Pending invitations the alliance has sent out (the management view, AC11).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn invites_of(&self, alliance: AllianceId) -> Result<Vec<OutgoingInvite>, RepoError>;

    /// Add `player` to `alliance` as `role`/`rights`, **guarded** on the alliance still being below
    /// `cap` (a single conditional insert, AC4). The `player_id` primary key rejects a player already in
    /// an alliance. Also deletes the consumed invitation in the same transaction.
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the alliance is at the cap; [`RepoError::Duplicate`] if the player is
    /// already in an alliance; [`RepoError`] otherwise.
    async fn add_member(
        &self,
        alliance: AllianceId,
        player: PlayerId,
        role: AllianceRole,
        rights: RightSet,
        cap: u32,
    ) -> Result<(), RepoError>;

    /// Remove `player` from their alliance (leave / expel).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn remove_member(&self, player: PlayerId) -> Result<(), RepoError>;

    /// Set `player`'s role + rights within `alliance` (promote/demote, grant/revoke — AC6).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn set_member_role(
        &self,
        alliance: AllianceId,
        player: PlayerId,
        role: AllianceRole,
        rights: RightSet,
    ) -> Result<(), RepoError>;

    /// Transfer the founder role in `alliance`: `to` becomes the **Founder**; the old founder `from`
    /// becomes an ordinary **Member**, in one transaction (AC5/AC6).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn transfer_founder(
        &self,
        alliance: AllianceId,
        from: PlayerId,
        to: PlayerId,
    ) -> Result<(), RepoError>;

    /// Disband `alliance`: delete it, cascading to its members, invitations, and diplomacy, in one
    /// transaction (AC5/AC12).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn disband(&self, alliance: AllianceId) -> Result<(), RepoError>;

    /// The current diplomacy state of the pair `(a, b)` (normalised internally), or `None` if Neutral:
    /// the stance, its status, and the proposer of a pending confederation (AC7).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn diplomacy_state(
        &self,
        a: AllianceId,
        b: AllianceId,
    ) -> Result<Option<(DiplomacyStance, DiplomacyStatus, Option<AllianceId>)>, RepoError>;

    /// Upsert the diplomacy state of the pair `(a, b)` (normalised internally) — the stance, status, and
    /// the proposer of a pending confederation (AC7).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn set_diplomacy_state(
        &self,
        a: AllianceId,
        b: AllianceId,
        stance: DiplomacyStance,
        status: DiplomacyStatus,
        proposed_by: Option<AllianceId>,
    ) -> Result<(), RepoError>;

    /// Clear the pair `(a, b)` back to Neutral (delete the row).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn clear_diplomacy(&self, a: AllianceId, b: AllianceId) -> Result<(), RepoError>;

    /// Every diplomacy relationship `alliance` holds, for its diplomacy page (AC7/AC11).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn diplomacy_of(&self, alliance: AllianceId) -> Result<Vec<DiplomacyEntry>, RepoError>;

    /// The alliances `alliance` is in an **active confederation** with — the one-hop confederate set the
    /// visibility/defence reads use (AC8/AC9).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn confederate_alliances(
        &self,
        alliance: AllianceId,
    ) -> Result<Vec<AllianceId>, RepoError>;

    /// Every village owned by a member of any of `alliances` — the shared village list (AC8). Ordered
    /// by owner then village. Coordinates + names only.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn alliance_member_villages(
        &self,
        alliances: &[AllianceId],
    ) -> Result<Vec<AlliedVillage>, RepoError>;

    /// In-transit **hostile** movements (attack / raid) whose target is one of `villages` — the
    /// incoming-defence view (AC9). Target + arrival time only; the attacker's troops are never joined
    /// (P4/§7.3). Ordered by arrival time.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn incoming_against(
        &self,
        villages: &[VillageId],
    ) -> Result<Vec<IncomingAttack>, RepoError>;

    // ---- Alliance forum (027). Default no-ops so non-forum fakes are untouched. ----

    /// Start a thread in `alliance` with a `title` + first post `body`; returns the thread id. Sets
    /// `last_post_at` to `now`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn create_thread(
        &self,
        _alliance: AllianceId,
        _author: PlayerId,
        _title: &str,
        _body: &str,
        _announcement: bool,
        _now: Timestamp,
    ) -> Result<u128, RepoError> {
        Ok(0)
    }

    /// An alliance's threads, most-recent activity first, capped at `limit` (027 AC1).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn list_threads(
        &self,
        _alliance: AllianceId,
        _limit: i64,
    ) -> Result<Vec<ThreadSummary>, RepoError> {
        Ok(Vec::new())
    }

    /// A thread's header (owning alliance + title + announcement flag), or `None` if it doesn't exist.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn thread_head(&self, _thread: u128) -> Result<Option<ThreadHead>, RepoError> {
        Ok(None)
    }

    /// Append a post to a thread + bump its `last_post_at`; returns the post id (027 AC3).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn add_post(
        &self,
        _thread: u128,
        _author: PlayerId,
        _body: &str,
        _now: Timestamp,
    ) -> Result<u128, RepoError> {
        Ok(0)
    }

    /// A thread's posts, oldest→newest, capped at `limit` (027 AC1).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn list_posts(&self, _thread: u128, _limit: i64) -> Result<Vec<ForumPost>, RepoError> {
        Ok(Vec::new())
    }

    /// Alliances whose **name or tag** is a case-insensitive prefix of `query` (028 AC2), capped at
    /// `limit`. Defaults to empty.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn search_alliances(
        &self,
        _query: &str,
        _limit: i64,
    ) -> Result<Vec<AllianceHit>, RepoError> {
        Ok(Vec::new())
    }
}

/// A claimed, due settle movement ready to resolve (013). Targets a free **valley tile**, not a village.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSettle {
    /// The movement's identity.
    pub id: u128,
    /// The settling player.
    pub owner: PlayerId,
    /// The source village (the settlers came from here / bounce back here).
    pub home_village: VillageId,
    /// The source tile.
    pub origin: Coordinate,
    /// The destination valley tile to found on.
    pub target: Coordinate,
    /// When the settlers arrive (the resolution instant).
    pub arrive_at: Timestamp,
    /// The settler group.
    pub troops: UnitCounts,
}

/// What a resolved settle does: **found** a new village on the target tile, or **bounce** the settlers
/// back home (the tile was taken or the slot was lost in flight).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettleOutcome {
    /// Found a new village (the repository assigns its id). `culture_value` is the player's culture
    /// accumulator settled to the resolution instant at the **old** rate (before the new village joins
    /// it) — written in the same founding transaction (013 AC1/P2).
    Found { culture_value: i64 },
    /// Bounce the settlers home as a `return` arriving at this instant.
    Bounce { return_arrive: Timestamp },
}

/// The single-transaction application of a resolved settle (013 AC6/AC7/AC12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettleApply {
    /// The settle movement to mark `done`.
    pub movement_id: u128,
    /// The settling player (and owner of a founded village).
    pub owner: PlayerId,
    /// The source village (the bounce return is delivered here).
    pub home_village: VillageId,
    /// The source tile (the bounce return's destination).
    pub home_coord: Coordinate,
    /// The target valley tile.
    pub target: Coordinate,
    /// The settler group (consumed on a found; returned on a bounce).
    pub troops: UnitCounts,
    /// The new/founding village's tribe (the player's).
    pub tribe: Tribe,
    /// The resolution instant (a bounce return departs then; the culture settle stamps it).
    pub battle_at: Timestamp,
    /// Found or bounce.
    pub outcome: SettleOutcome,
}

/// Persistence for settling (013): dispatch settlers, claim due settles, found-or-bounce.
#[async_trait]
pub trait SettleRepository: Send + Sync {
    /// Atomically debit the settler group from the `home` garrison (guarded) and create a `settle`
    /// movement to the `target` valley tile (no destination village) arriving at `arrive_at`.
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if the garrison no longer covers the settlers; [`RepoError`] otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn start_settle(
        &self,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        target: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError>;

    /// Atomically claim settle movements whose arrival is due (`in_transit → processing`).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn claim_due_settles(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueSettle>, RepoError>;

    /// Apply a resolved settle in **one** transaction. **Found**: insert the new village at the target
    /// (the seeded tile's fields + `template` buildings + starting resources, owned by `owner`) and
    /// re-anchor the player's culture; the founding is guarded on the tile still being free
    /// ([`RepoError::Conflict`] if taken in flight). **Bounce**: schedule the settlers' `return` home.
    /// Either way, mark the movement `done`. Exactly-once with the orphan requeue (AC12).
    ///
    /// # Errors
    /// [`RepoError::Conflict`] if a `Found` target was taken in flight; [`RepoError`] otherwise.
    async fn apply_settle(
        &self,
        apply: SettleApply,
        template: &StartingVillage,
    ) -> Result<(), RepoError>;
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
    /// When the order completed (its due instant, Unix-ms UTC) — the deterministic time a rate-changing
    /// completion (e.g. a Town Hall) re-anchors the player's culture at, independent of when the
    /// scheduler fires (013 AC1/P2).
    pub complete_at: Timestamp,
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

/// The scope of a leaderboard query (016 AC7): the whole world, or one map quadrant (filtered by the
/// player's / alliance member's capital).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardScope {
    /// Every player/alliance in the world.
    World,
    /// Only those whose capital lies in the given quadrant.
    Quadrant(Quadrant),
}

/// Which conflict metric a windowed board ranks by (016 AC5/AC6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictMetric {
    /// Attack points — valued defender troops destroyed on the offensive.
    Attack,
    /// Defense points — the defender's split of valued attacker troops destroyed.
    Defense,
    /// Resources looted as the attacker (summed across the four resources).
    Raided,
}

/// One ranked player on a leaderboard (016): the player, their public name, and the metric value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderboardRow {
    pub player: PlayerId,
    pub name: String,
    pub value: i64,
    /// The player's last activity (Unix-ms UTC) — feeds the 025 presence indicator on board rows.
    pub last_activity: Timestamp,
}

/// One ranked alliance on a leaderboard (016 AC8): the alliance, its name + tag, and the aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllianceLeaderboardRow {
    pub alliance: AllianceId,
    pub name: String,
    pub tag: String,
    pub value: i64,
}

/// A defender's own report for a battle they had troops in (016 AC3/AC12) — the reinforcer inbox row
/// and a line of a player's stat-page battle history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefenderReport {
    /// The battle this contribution belongs to.
    pub battle_id: u128,
    pub occurred_at: Timestamp,
    /// The defending village these troops were stationed at (the owner's village).
    pub at_village: VillageId,
    /// Whether the viewer owned the defending village (vs. having reinforced it).
    pub is_owner: bool,
    /// The viewer's troops in the battle, and their losses.
    pub forces: UnitCounts,
    pub losses: UnitCounts,
    /// The defense points the viewer earned.
    pub defense_points: i64,
}

/// A player's public statistics (016 AC9): the headline metrics, derived from persisted state. Never
/// includes private troop counts or stored resources (P4/§7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStats {
    pub player: PlayerId,
    pub name: String,
    /// Total population across all the player's villages.
    pub population: i64,
    /// Public per-village breakdown: (village, coordinate, population) — villages are public by tile.
    pub villages: Vec<(VillageId, Coordinate, i64)>,
    pub attack_points: i64,
    pub defense_points: i64,
    /// Total resources looted as attacker (the four resources summed).
    pub loot_total: i64,
}

/// An alliance's public statistics (016 AC10): aggregates over current members + their contributions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllianceStats {
    pub alliance: AllianceId,
    pub name: String,
    pub tag: String,
    pub population: i64,
    pub attack_points: i64,
    pub defense_points: i64,
    /// Per-member public contribution: (player, name, population, attack points, defense points).
    pub members: Vec<(PlayerId, String, i64, i64, i64)>,
}

/// Read-only ranking & statistics queries (016): public leaderboards and stat pages, derived on read
/// from persisted facts (population from build state; points/loot from the battle rows). Every board
/// is bounded by a caller-supplied `limit` (P11); `since` is the window lower bound (`None` =
/// all-time). Quadrant scope filters by the player's / member's capital (AC7).
#[async_trait]
pub trait RankingRepository: Send + Sync {
    /// Players ranked by total population (AC1/AC2). Population is summed from current build levels
    /// using `econ`'s per-level tables.
    async fn population_board(
        &self,
        econ: &EconomyRules,
        scope: BoardScope,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, RepoError>;

    /// Players ranked by a conflict metric over `[since, until)` (AC5/AC6; either bound `None` is
    /// open); zero-activity players omitted. `until` bounds the period for a reproducible settlement
    /// (017) even when catching up late.
    async fn conflict_board(
        &self,
        metric: ConflictMetric,
        scope: BoardScope,
        since: Option<Timestamp>,
        until: Option<Timestamp>,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, RepoError>;

    /// Alliances ranked by aggregate member population (AC8).
    async fn alliance_population_board(
        &self,
        econ: &EconomyRules,
        scope: BoardScope,
        limit: i64,
    ) -> Result<Vec<AllianceLeaderboardRow>, RepoError>;

    /// Alliances ranked by aggregate member attack/defense points over `[since, until)` (AC8).
    async fn alliance_conflict_board(
        &self,
        metric: ConflictMetric,
        scope: BoardScope,
        since: Option<Timestamp>,
        until: Option<Timestamp>,
        limit: i64,
    ) -> Result<Vec<AllianceLeaderboardRow>, RepoError>;

    /// A player's public statistics (AC9), or `None` if the player does not exist.
    async fn player_stats(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
    ) -> Result<Option<PlayerStats>, RepoError>;

    /// An alliance's public statistics (AC10), or `None` if the alliance does not exist.
    async fn alliance_stats(
        &self,
        econ: &EconomyRules,
        alliance: AllianceId,
    ) -> Result<Option<AllianceStats>, RepoError>;

    /// The defender reports a player earned (AC3/AC12) — battles where they owned the target or
    /// reinforced it — newest first, bounded by `limit`.
    async fn defender_reports_for(
        &self,
        player: PlayerId,
        limit: i64,
    ) -> Result<Vec<DefenderReport>, RepoError>;
}

/// Whether a medal's subject is a player or an alliance (017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MedalSubjectKind {
    Player,
    Alliance,
}

impl MedalSubjectKind {
    /// The persisted string key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Alliance => "alliance",
        }
    }
}

/// A medal to award at settlement (017 AC3): a category + rank to a player or alliance subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MedalAward {
    pub category: MedalCategory,
    pub rank: usize,
    pub subject_kind: MedalSubjectKind,
    /// The subject's id (a `PlayerId.0` or `AllianceId.0`).
    pub subject_id: u128,
}

/// A medal a subject holds (017 AC5), newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MedalView {
    pub period: i64,
    pub category: MedalCategory,
    pub rank: i64,
}

/// Read/write of the weekly-settlement state: population snapshots and medals (017). The repository is
/// bound to one world; `period` is the settlement period index (`domain::medals::period_index`).
#[async_trait]
pub trait MedalRepository: Send + Sync {
    /// The most recent settled period for this world (the max snapshot period), or `None` if never.
    async fn latest_settled_period(&self) -> Result<Option<i64>, RepoError>;

    /// Write a population snapshot for **every** player for `period` (idempotent — re-running writes
    /// none). Population is computed from current build state using `econ`'s per-level tables.
    ///
    /// **Note:** the production settlement uses [`MedalRepository::settle_period`], which snapshots and
    /// awards medals in one transaction. This standalone snapshot advances the `MAX(period)` watermark
    /// by itself, so it must not drive a settlement (that would reintroduce the non-atomic bug) — it is
    /// for tests/diagnostics.
    async fn snapshot_population(&self, econ: &EconomyRules, period: i64) -> Result<(), RepoError>;

    /// Award the given medals for `period` (idempotent via the per-period (category, rank) uniqueness).
    async fn award_medals(&self, period: i64, awards: &[MedalAward]) -> Result<(), RepoError>;

    /// Settle a period **atomically** (017 AC6/P2): in one transaction, write the population snapshot,
    /// compute the **climber** medals from that just-written snapshot (when `climber_limit` is `Some`),
    /// and award them together with `awards` (the pre-computed non-climber medals). Because the
    /// snapshot advances the `MAX(period)` watermark, it must commit *with* the medals — so a failure
    /// re-settles the whole period rather than losing its medals. Idempotent (snapshot PK + medal
    /// uniqueness).
    async fn settle_period(
        &self,
        econ: &EconomyRules,
        period: i64,
        climber_limit: Option<i64>,
        awards: &[MedalAward],
    ) -> Result<(), RepoError>;

    /// A subject's held medals (017 AC5), newest first.
    async fn medals_for(
        &self,
        subject_kind: MedalSubjectKind,
        subject_id: u128,
    ) -> Result<Vec<MedalView>, RepoError>;

    /// Players ranked by population gained from `prev` to `period` (the climber metric, 017 AC4 / the
    /// 016-deferred board). Only positive gainers; quadrant-scoped; bounded by `limit`.
    async fn climber_board(
        &self,
        period: i64,
        prev: i64,
        scope: BoardScope,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, RepoError>;

    /// A player's population snapshots over time as `(period, population)`, oldest first (AC11).
    async fn population_history(&self, player: PlayerId) -> Result<Vec<(i64, i64)>, RepoError>;
}

/// Read/write of achievement progress + grants (017). The repository is bound to one world.
#[async_trait]
pub trait AchievementRepository: Send + Sync {
    /// The achievements a player already holds (017 AC8) — the idempotency set.
    async fn held_achievements(
        &self,
        player: PlayerId,
    ) -> Result<HashSet<AchievementId>, RepoError>;

    /// A player's current persisted progress (counts) for the achievement predicates. `tribe_unit_count`
    /// is left 0 — the caller fills it from the unit roster (the DB doesn't know roster sizes).
    async fn player_progress(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
    ) -> Result<PlayerProgress, RepoError>;

    /// Grant `def` to `player` and apply its reward (resources to the capital, capped; or culture
    /// points) in **one transaction**, exactly once (the `(player, achievement)` PK guards against
    /// double grant/reward). Returns `true` if newly granted.
    async fn grant_achievement(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
        def: &AchievementDef,
    ) -> Result<bool, RepoError>;
}

/// Read/write of onboarding quest progress (018). The repository is bound to one world.
#[async_trait]
pub trait QuestRepository: Send + Sync {
    /// The quests a player has completed (018 AC3) — the chain-progress set.
    async fn completed_quests(&self, player: PlayerId) -> Result<HashSet<QuestId>, RepoError>;

    /// The player's persisted facts the quest conditions read (max field level, per-building max level,
    /// any troops, any raid launched, population).
    async fn quest_progress(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
    ) -> Result<QuestProgress, RepoError>;

    /// Complete `def` for `player` and apply its reward (resources to the capital capped / culture /
    /// troops to the capital garrison) in **one transaction**, exactly once (the `(player, quest)` PK
    /// guards against double completion/reward). Returns `true` if newly completed.
    async fn complete_quest(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
        def: &QuestDef,
    ) -> Result<bool, RepoError>;
}

/// Persistence for the inactivity → abandonment sweep (019). State-driven like the 017 settlement: the
/// latest swept period is a watermark, and each period is settled atomically and idempotently.
#[async_trait]
pub trait LifecycleRepository: Send + Sync {
    /// The latest abandonment-sweep period already settled for this world (`MAX(inactivity_sweeps.
    /// period)`), or `None` if none yet.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn latest_swept_period(&self) -> Result<Option<i64>, RepoError>;

    /// Settle abandonment for `period`: in **one transaction**, record the period watermark and, for
    /// every live account whose `last_activity` is before `cutoff`, delete its villages (freeing the
    /// valleys) and flag it `abandoned`. Returns the number of accounts abandoned. Idempotent — a
    /// re-settle of a recorded period is a no-op (the watermark guards it; already-abandoned accounts
    /// are excluded regardless).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn sweep_abandoned(&self, period: i64, cutoff: Timestamp) -> Result<usize, RepoError>;
}

/// One held artifact and the village holding it (020).
#[derive(Debug, Clone, PartialEq)]
pub struct HeldArtifact {
    /// The artifact.
    pub def: ArtifactDef,
    /// The village currently holding it.
    pub holder: VillageId,
}

/// Persistence for the end-game artifacts + their Natar vaults (020).
#[async_trait]
pub trait ArtifactRepository: Send + Sync {
    /// Materialize the released artifact set **once** (020 AC1): if `now ≥ release_at` and nothing has
    /// been released yet, ensure the synthetic Natar NPC owner, place one Natar village per artifact on
    /// the reserved Natar tiles (seeded ring order) with a seeded garrison, and insert each artifact
    /// held by its Natar village. Returns the number released (0 if not yet due or already released).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn release_artifacts(
        &self,
        release_at: Timestamp,
        now: Timestamp,
        catalogue: &[ArtifactDef],
        garrison_unit: &str,
        garrison_base_count: i64,
        garrison_per_index: i64,
    ) -> Result<usize, RepoError> {
        let _ = (
            release_at,
            now,
            catalogue,
            garrison_unit,
            garrison_base_count,
            garrison_per_index,
        );
        Ok(0)
    }

    /// The artifact a village currently holds, if any (020). Defaults to none; the real adapter
    /// overrides it (so non-artifact fakes need not implement it).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn artifact_at_village(
        &self,
        _village: VillageId,
    ) -> Result<Option<ArtifactDef>, RepoError> {
        Ok(None)
    }

    /// Every artifact a player currently holds, with its holding village (020) — for effects + display.
    /// Defaults to empty; the real adapter overrides it.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn held_by_player(&self, _player: PlayerId) -> Result<Vec<HeldArtifact>, RepoError> {
        Ok(Vec::new())
    }
}

/// One alliance's Wonder progress (021) — for the race board (T8) and the victory check (T7).
#[derive(Debug, Clone, PartialEq)]
pub struct WonderStanding {
    /// The alliance whose member holds the highest Wonder.
    pub alliance: AllianceId,
    /// The alliance tag (display).
    pub tag: String,
    /// The alliance name (display).
    pub name: String,
    /// Its highest Wonder level across its members' sites (0..=100).
    pub level: u8,
}

/// The round's recorded outcome (021 AC6) — the winning alliance and when it won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WonderOutcome {
    /// The first alliance to a complete Wonder.
    pub winner: AllianceId,
    /// When victory was recorded.
    pub won_at: Timestamp,
}

/// Persistence for the end-game Wonder of the World, its plans + conquerable sites, and the round
/// result (021). Default no-ops/empties so non-Wonder fakes need not implement it.
#[async_trait]
pub trait WonderRepository: Send + Sync {
    /// Materialize the Wonder release **once** (021 AC1): if `now ≥ release_at` and nothing has been
    /// released yet, ensure the synthetic Natar owner, place `site_count` **conquerable** Wonder-site
    /// Natar villages and `plan_count` capturable plan vaults (each garrisoned, on reserved Natar tiles
    /// in seeded ring order). Returns the count materialized (0 if not due or already released).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    #[allow(clippy::too_many_arguments)]
    async fn release_wonder(
        &self,
        release_at: Timestamp,
        now: Timestamp,
        plan_count: u32,
        site_count: u32,
        garrison_unit: &str,
        garrison_base_count: i64,
        garrison_per_index: i64,
    ) -> Result<usize, RepoError> {
        let _ = (
            release_at,
            now,
            plan_count,
            site_count,
            garrison_unit,
            garrison_base_count,
            garrison_per_index,
        );
        Ok(0)
    }

    /// The id of the Wonder plan a village currently holds, if any (021). Defaults to none.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn plan_at_village(&self, _village: VillageId) -> Result<Option<String>, RepoError> {
        Ok(None)
    }

    /// Whether any member of `alliance` currently holds a Wonder plan (gates construction, 021 AC4).
    /// Defaults to `false`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn alliance_holds_plan(&self, _alliance: AllianceId) -> Result<bool, RepoError> {
        Ok(false)
    }

    /// The Wonder level built at `village` (0 if none). Defaults to 0.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn wonder_level(&self, _village: VillageId) -> Result<u8, RepoError> {
        Ok(0)
    }

    /// The alliances with Wonder progress, highest first (021 AC9). Defaults to empty.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn top_wonders(&self) -> Result<Vec<WonderStanding>, RepoError> {
        Ok(Vec::new())
    }

    /// The round outcome if the world has been won, else `None` (021 AC6/AC7). Defaults to `None`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn world_ended(&self) -> Result<Option<WonderOutcome>, RepoError> {
        Ok(None)
    }

    /// Record `winner` as the round's victor at `won_at` **once** (021 AC6): no-op if already won.
    /// Returns `true` if this call recorded the victory. Defaults to `false` (no-op).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn record_victory(
        &self,
        _winner: AllianceId,
        _won_at: Timestamp,
    ) -> Result<bool, RepoError> {
        Ok(false)
    }
}

/// One open report on the moderator review queue (022 AC3): identity + the parties' names for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportView {
    /// The report's id (the resolve target).
    pub id: u128,
    /// The reporting player + their name.
    pub reporter: PlayerId,
    pub reporter_name: String,
    /// The reported account + its name.
    pub subject: PlayerId,
    pub subject_name: String,
    /// The reason + free-text note.
    pub reason: ReportReason,
    pub note: String,
    /// When the report was filed (Unix-ms UTC) — the queue orders by this, oldest first.
    pub created_ms: i64,
}

/// Persistence for fair-play moderation (022): the Moderator role, player reports, sanctions, the
/// rate-limit counters, and the detection signals. Default no-ops/empties so non-moderation fakes are
/// untouched.
#[async_trait]
pub trait ModerationRepository: Send + Sync {
    /// Set (or clear) the elevated Moderator role on an account (022 AC1) — used by the operator
    /// bootstrap. Idempotent.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn set_moderator(&self, _player: PlayerId, _is_moderator: bool) -> Result<(), RepoError> {
        Ok(())
    }

    /// Record the registration IP for an account (022) — the shared-IP detection key, captured at
    /// register. Idempotent (last write wins).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn record_registration_ip(&self, _player: PlayerId, _ip: &str) -> Result<(), RepoError> {
        Ok(())
    }

    /// File a report of `subject` by `reporter` with a reason + note (022 AC2). Returns `true` if a new
    /// open report was created, `false` if a duplicate **open** report already existed (collapsed).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn file_report(
        &self,
        _reporter: PlayerId,
        _subject: PlayerId,
        _reason: ReportReason,
        _note: &str,
    ) -> Result<bool, RepoError> {
        Ok(false)
    }

    /// The open reports, **oldest first**, up to `limit` (022 AC3).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn open_reports(&self, _limit: i64) -> Result<Vec<ReportView>, RepoError> {
        Ok(Vec::new())
    }

    /// Resolve an **open** report by `moderator` at `now`, recording the `resolution`, and — in the same
    /// transaction — apply an optional sanction to the report's subject (022 AC4). Returns `true` if this
    /// call resolved it, `false` if it was already resolved / not found (idempotent).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn resolve_report(
        &self,
        _report_id: u128,
        _moderator: PlayerId,
        _now: Timestamp,
        _resolution: &str,
        _sanction_kind: Option<SanctionKind>,
        _suspended_until: Option<Timestamp>,
    ) -> Result<bool, RepoError> {
        Ok(false)
    }

    /// Apply a sanction to `subject` directly (022 AC4) — a **ban** stamps `banned_at = now`, a
    /// **suspend** sets `suspended_until`, a **warn** changes no block state.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn apply_sanction(
        &self,
        _subject: PlayerId,
        _now: Timestamp,
        _kind: SanctionKind,
        _suspended_until: Option<Timestamp>,
    ) -> Result<(), RepoError> {
        Ok(())
    }

    /// How many accounts share `subject`'s registration IP, including itself (022 AC7); 0 if the IP is
    /// unknown.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn ip_association_count(&self, _subject: PlayerId) -> Result<u32, RepoError> {
        Ok(0)
    }

    /// The peak per-window action count recorded for `subject` (022 AC7) — the inhuman-action-rate
    /// signal's reproducible input.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn peak_action_count(&self, _subject: PlayerId) -> Result<u32, RepoError> {
        Ok(0)
    }

    /// Atomically bump the fixed-window counter for `(subject, action)` at the window containing `now`
    /// (window length `window_secs`) and return the **new** count (022 AC6). The web rate-limit guard
    /// compares it to the configured limit.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn bump_rate(
        &self,
        _subject: &str,
        _action: &str,
        _now: Timestamp,
        _window_secs: i64,
    ) -> Result<u32, RepoError> {
        Ok(0)
    }
}

/// One message line in a conversation (024) — a DM line or a channel line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageView {
    /// Message id.
    pub id: u128,
    /// Sender identity + display name.
    pub sender: PlayerId,
    pub sender_name: String,
    /// Body text.
    pub body: String,
    /// When sent (Unix-ms UTC).
    pub created_ms: i64,
}

/// A row in the conversations list (024 AC3): a DM thread or a channel, with its latest line + unread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSummary {
    /// The viewer-relative key (`dm:<other>` / `global` / `alliance:<id>`).
    pub key: String,
    /// Display title (the other player's name, or the channel name).
    pub title: String,
    /// The latest message's body + time (empty/0 if the thread has no messages yet).
    pub last_body: String,
    pub last_ms: i64,
    /// Unread count for the viewer.
    pub unread: i64,
    /// For DM threads, the other party's last activity (Unix-ms UTC) for the 025 presence indicator;
    /// `None` for channels (which have no single presence).
    pub other_last_activity: Option<i64>,
}

/// Persistence for conversations (024): direct messages, channel chat, and per-viewer read state. Default
/// no-ops so non-comms fakes are untouched.
#[async_trait]
pub trait CommsRepository: Send + Sync {
    /// Persist a direct message (validation/recipient checks are the use-case's). Returns its id.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn send_dm(
        &self,
        _sender: PlayerId,
        _recipient: PlayerId,
        _body: &str,
        _now: Timestamp,
    ) -> Result<u128, RepoError> {
        Ok(0)
    }

    /// The DM thread between `viewer` and `other` (both directions), oldest→newest, last `limit`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn dm_history(
        &self,
        _viewer: PlayerId,
        _other: PlayerId,
        _limit: i64,
    ) -> Result<Vec<MessageView>, RepoError> {
        Ok(Vec::new())
    }

    /// `viewer`'s DM threads (one per other party), each with the latest line + unread, newest first.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn dm_threads(&self, _viewer: PlayerId) -> Result<Vec<ConversationSummary>, RepoError> {
        Ok(Vec::new())
    }

    /// Persist a channel message (access checks are the use-case's). Returns its id.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn post_chat(
        &self,
        _channel_key: &str,
        _sender: PlayerId,
        _body: &str,
        _now: Timestamp,
    ) -> Result<u128, RepoError> {
        Ok(0)
    }

    /// A channel's recent history, oldest→newest, last `limit`.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn chat_history(
        &self,
        _channel_key: &str,
        _limit: i64,
    ) -> Result<Vec<MessageView>, RepoError> {
        Ok(Vec::new())
    }

    /// The latest line + time of a channel (for the conversations list), or `None` if empty.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn channel_latest(&self, _channel_key: &str) -> Result<Option<(String, i64)>, RepoError> {
        Ok(None)
    }

    /// Advance `player`'s read watermark for `conversation` to `now` (024 AC4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn mark_read(
        &self,
        _player: PlayerId,
        _conversation: &str,
        _now: Timestamp,
    ) -> Result<(), RepoError> {
        Ok(())
    }

    /// Unread count for `player` in a channel `conversation` (messages after their watermark not their own).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn channel_unread(
        &self,
        _player: PlayerId,
        _channel_key: &str,
    ) -> Result<i64, RepoError> {
        Ok(0)
    }

    /// `player`'s total unread across **all** their DM threads, in one query (the nav badge, 024 AC4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn dm_total_unread(&self, _player: PlayerId) -> Result<i64, RepoError> {
        Ok(0)
    }
}

/// A notification to persist (026): the recipient + the kind + an optional deep-link reference + a short
/// pre-rendered body. Built at the event-commit sites; `record`ed in bulk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotification {
    /// The recipient (the notification is private to them).
    pub player: PlayerId,
    /// The kind (its stable token is persisted).
    pub kind: NotificationKind,
    /// Optional pointer the UI dereferences (`report` / `dm` / `village`), with its id/coords.
    pub ref_kind: Option<String>,
    pub ref_id: Option<String>,
    /// A short pre-rendered detail line for the feed.
    pub body: String,
}

/// A persisted notification as shown in the feed (026 AC4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationView {
    pub id: u128,
    pub kind: NotificationKind,
    pub ref_kind: Option<String>,
    pub ref_id: Option<String>,
    pub body: String,
    /// When recorded (Unix-ms UTC).
    pub created_ms: i64,
    /// Whether the recipient has read it.
    pub read: bool,
}

/// Persistence for notifications (026): a per-player feed delivered live via `LISTEN/NOTIFY`. Default
/// no-ops so non-notification fakes are untouched.
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Persist a batch of notifications and emit a per-recipient `pg_notify('notifications', …)` live nudge
    /// in the same statement (persist-then-notify; the row is the source of truth, 026 AC6). A no-op for an
    /// empty batch.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn record(&self, _notes: &[NewNotification], _now: Timestamp) -> Result<(), RepoError> {
        Ok(())
    }

    /// `player`'s notifications, most-recent first, capped at `limit` (026 AC4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn list(
        &self,
        _player: PlayerId,
        _limit: i64,
    ) -> Result<Vec<NotificationView>, RepoError> {
        Ok(Vec::new())
    }

    /// `player`'s unread notification count — the nav bell (026 AC4).
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn unread_count(&self, _player: PlayerId) -> Result<i64, RepoError> {
        Ok(0)
    }

    /// Mark all of `player`'s unread notifications read at `now` (026 AC5). Owner-scoped by the `player`
    /// argument — a player can only ever clear their own.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn mark_read(&self, _player: PlayerId, _now: Timestamp) -> Result<(), RepoError> {
        Ok(())
    }

    /// The kinds `player` has **muted** (029) — absent kinds are enabled (default-on). Defaults to empty.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn muted_kinds(&self, _player: PlayerId) -> Result<Vec<NotificationKind>, RepoError> {
        Ok(Vec::new())
    }

    /// Mute (`muted = true`) or un-mute a notification kind for `player` (029) — idempotent. Owner-scoped.
    ///
    /// # Errors
    /// [`RepoError::Backend`] on storage failure.
    async fn set_mute(
        &self,
        _player: PlayerId,
        _kind: NotificationKind,
        _muted: bool,
    ) -> Result<(), RepoError> {
        Ok(())
    }
}
