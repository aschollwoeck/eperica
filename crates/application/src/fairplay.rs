//! Fair-play / anti-cheat use-cases (022): player reporting, the moderator review queue + sanctioning,
//! the detection-signal aggregation, and the rate-limit check. All enforcement is server-authoritative
//! (P4) and gated on the elevated Moderator role; thresholds come from [`FairPlayRules`] (P7).

use crate::ports::{AccountRepository, ModerationRepository, RepoError, ReportView};
use eperica_domain::{
    FairPlayRules, PlayerId, ReportReason, SanctionKind, Timestamp, inhuman_action_rate,
    shared_ip_flagged,
};

/// Why a moderation action was rejected (022).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModerationError {
    /// The actor is not a moderator (a moderator-only action), or a player tried to act on themselves.
    #[error("not authorized")]
    NotAuthorized,
    /// A player tried to report their own account.
    #[error("cannot report your own account")]
    SelfReport,
    /// The target report/account does not exist.
    #[error("not found")]
    NotFound,
    /// The subject exceeded the configured rate limit (022 AC6).
    #[error("rate limited")]
    RateLimited,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for ModerationError {
    fn from(e: RepoError) -> Self {
        ModerationError::Backend(e.to_string())
    }
}

/// The two reproducible detection signals for an account (022 AC7), as advisory inputs to a moderator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountSignals {
    /// Accounts sharing this account's registration IP (including itself).
    pub ip_association_count: u32,
    /// Whether that count crosses the shared-IP threshold.
    pub shared_ip_flagged: bool,
    /// The peak per-window action count recorded for the account.
    pub peak_action_count: u32,
    /// Whether that count crosses the inhuman-action-rate threshold.
    pub inhuman_action_rate: bool,
}

/// Whether `actor` holds the Moderator role (022 AC1) — the gate for every moderator-only action.
async fn require_moderator<A>(accounts: &A, actor: PlayerId) -> Result<(), ModerationError>
where
    A: AccountRepository,
{
    match accounts.find_user_by_id(actor).await? {
        Some(u) if u.is_moderator => Ok(()),
        _ => Err(ModerationError::NotAuthorized),
    }
}

/// File a report of `subject` by `reporter` (022 AC2). A self-report is rejected; a duplicate **open**
/// report by the same reporter against the same subject is collapsed (no new row). Returns `true` if a
/// new report was created.
///
/// # Errors
/// [`ModerationError::SelfReport`] for a self-report; otherwise a backend error.
pub async fn file_report<M>(
    moderation: &M,
    reporter: PlayerId,
    subject: PlayerId,
    reason: ReportReason,
    note: &str,
) -> Result<bool, ModerationError>
where
    M: ModerationRepository,
{
    if reporter == subject {
        return Err(ModerationError::SelfReport);
    }
    Ok(moderation
        .file_report(reporter, subject, reason, note)
        .await?)
}

/// The moderator review queue — open reports, oldest first (022 AC3). Moderator-gated.
///
/// # Errors
/// [`ModerationError::NotAuthorized`] for a non-moderator; otherwise a backend error.
pub async fn review_queue<A, M>(
    accounts: &A,
    moderation: &M,
    actor: PlayerId,
    limit: i64,
) -> Result<Vec<ReportView>, ModerationError>
where
    A: AccountRepository,
    M: ModerationRepository,
{
    require_moderator(accounts, actor).await?;
    Ok(moderation.open_reports(limit).await?)
}

/// Resolve a report and optionally sanction its subject in one action (022 AC4). Moderator-gated;
/// idempotent (resolving an already-resolved report is a no-op returning `false`). For a **suspend**
/// without an explicit `suspend_until`, the window defaults to `rules.suspend_default_secs` from `now`.
///
/// # Errors
/// [`ModerationError::NotAuthorized`] for a non-moderator; otherwise a backend error.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_report<A, M>(
    accounts: &A,
    moderation: &M,
    rules: &FairPlayRules,
    actor: PlayerId,
    report_id: u128,
    now: Timestamp,
    resolution: &str,
    sanction: Option<SanctionKind>,
    suspend_until: Option<Timestamp>,
) -> Result<bool, ModerationError>
where
    A: AccountRepository,
    M: ModerationRepository,
{
    require_moderator(accounts, actor).await?;
    let suspended_until = sanction_window(sanction, suspend_until, rules, now);
    Ok(moderation
        .resolve_report(report_id, actor, now, resolution, sanction, suspended_until)
        .await?)
}

/// Apply a sanction to an account directly, from the account-inspect page (022 AC4). Moderator-gated.
///
/// # Errors
/// [`ModerationError::NotAuthorized`] for a non-moderator; otherwise a backend error.
#[allow(clippy::too_many_arguments)]
pub async fn sanction_account<A, M>(
    accounts: &A,
    moderation: &M,
    rules: &FairPlayRules,
    actor: PlayerId,
    subject: PlayerId,
    now: Timestamp,
    kind: SanctionKind,
    suspend_until: Option<Timestamp>,
) -> Result<(), ModerationError>
where
    A: AccountRepository,
    M: ModerationRepository,
{
    require_moderator(accounts, actor).await?;
    let suspended_until = sanction_window(Some(kind), suspend_until, rules, now);
    moderation
        .apply_sanction(subject, now, kind, suspended_until)
        .await?;
    Ok(())
}

/// The concrete suspension expiry for a sanction: a suspend uses the explicit window, else the config
/// default from `now`; non-suspend sanctions carry no expiry.
fn sanction_window(
    sanction: Option<SanctionKind>,
    explicit: Option<Timestamp>,
    rules: &FairPlayRules,
    now: Timestamp,
) -> Option<Timestamp> {
    if sanction == Some(SanctionKind::Suspend) {
        Some(explicit.unwrap_or(Timestamp(now.0 + rules.suspend_default_secs * 1000)))
    } else {
        None
    }
}

/// The detection signals for an account (022 AC7), computed deterministically from persisted state.
/// Moderator-gated.
///
/// # Errors
/// [`ModerationError::NotAuthorized`] for a non-moderator; otherwise a backend error.
pub async fn account_signals<A, M>(
    accounts: &A,
    moderation: &M,
    rules: &FairPlayRules,
    actor: PlayerId,
    subject: PlayerId,
) -> Result<AccountSignals, ModerationError>
where
    A: AccountRepository,
    M: ModerationRepository,
{
    require_moderator(accounts, actor).await?;
    let ip = moderation.ip_association_count(subject).await?;
    let peak = moderation.peak_action_count(subject).await?;
    Ok(AccountSignals {
        ip_association_count: ip,
        shared_ip_flagged: shared_ip_flagged(ip, rules),
        peak_action_count: peak,
        inhuman_action_rate: inhuman_action_rate(peak, rules),
    })
}

/// Count a request against the fixed-window rate limit for `(subject, action)` and reject it if the new
/// window count exceeds `limit` (022 AC6). Server-authoritative — the caller (a web guard) maps the
/// error to HTTP 429.
///
/// # Errors
/// [`ModerationError::RateLimited`] when over the limit; otherwise a backend error.
pub async fn check_rate_limit<M>(
    moderation: &M,
    rules: &FairPlayRules,
    subject: &str,
    action: &str,
    limit: u32,
    now: Timestamp,
) -> Result<(), ModerationError>
where
    M: ModerationRepository,
{
    let count = moderation
        .bump_rate(subject, action, now, rules.rate_window_secs)
        .await?;
    if count > limit {
        return Err(ModerationError::RateLimited);
    }
    Ok(())
}
