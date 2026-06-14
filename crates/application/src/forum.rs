//! Alliance-forum use-cases (027): read the thread list, open a thread, start a thread, and reply. Every
//! action loads the actor's [`Membership`] and is gated on it (P4): a player only ever reads/posts in their
//! own alliance's forum. Announcement threads require the 015 `Announce` right and are locked to replies.

use crate::ports::{AllianceRepository, ForumPost, RepoError, ThreadHead, ThreadSummary};
use eperica_domain::{
    AllianceRight, PlayerId, Timestamp, has_right, valid_body, valid_thread_title,
};

/// Page size for the thread list and a thread's posts (P11 — bounded reads).
pub const THREAD_LIMIT: i64 = 100;
pub const POST_LIMIT: i64 = 200;

/// Why a forum action was rejected (027, server-enforced — P4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForumError {
    /// The actor is not in any alliance.
    #[error("not a member of an alliance")]
    NotAMember,
    /// The actor lacks the right required (an announcement needs `Announce`).
    #[error("you lack the right for this action")]
    MissingRight,
    /// The thread does not exist, or is not in the actor's alliance (scope isolation — AC5).
    #[error("thread not found")]
    NotFound,
    /// The thread is an announcement (locked to replies — AC3).
    #[error("this thread is locked")]
    Locked,
    /// The title or body failed validation (AC6).
    #[error("invalid input")]
    Invalid,
    /// A backend/storage failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for ForumError {
    fn from(e: RepoError) -> Self {
        ForumError::Backend(e.to_string())
    }
}

/// The actor's alliance forum thread list (027 AC1), most-recent activity first. Members only.
///
/// # Errors
/// [`ForumError::NotAMember`], or a backend error.
pub async fn list_forum<R>(repo: &R, viewer: PlayerId) -> Result<Vec<ThreadSummary>, ForumError>
where
    R: AllianceRepository,
{
    let m = repo
        .alliance_of(viewer)
        .await?
        .ok_or(ForumError::NotAMember)?;
    Ok(repo.list_threads(m.alliance, THREAD_LIMIT).await?)
}

/// Open a thread the actor may read (027 AC1) — it must belong to the actor's alliance (AC5). Returns the
/// header (title + locked flag) + the posts (oldest→newest).
///
/// # Errors
/// [`ForumError::NotAMember`], [`ForumError::NotFound`], or a backend error.
pub async fn open_thread<R>(
    repo: &R,
    viewer: PlayerId,
    thread: u128,
) -> Result<(ThreadHead, Vec<ForumPost>), ForumError>
where
    R: AllianceRepository,
{
    let m = repo
        .alliance_of(viewer)
        .await?
        .ok_or(ForumError::NotAMember)?;
    let head = repo
        .thread_head(thread)
        .await?
        .ok_or(ForumError::NotFound)?;
    // Scope isolation (AC5): a thread is only reachable by members of its owning alliance.
    if head.alliance != m.alliance {
        return Err(ForumError::NotFound);
    }
    let posts = repo.list_posts(thread, POST_LIMIT).await?;
    Ok((head, posts))
}

/// Start a thread in the actor's alliance with a title + first post (027 AC2). An **announcement** requires
/// the `Announce` right (AC4). Returns the new thread id.
///
/// # Errors
/// [`ForumError::NotAMember`], [`ForumError::MissingRight`], [`ForumError::Invalid`], or a backend error.
pub async fn start_thread<R>(
    repo: &R,
    viewer: PlayerId,
    title: &str,
    body: &str,
    announcement: bool,
    now: Timestamp,
) -> Result<u128, ForumError>
where
    R: AllianceRepository,
{
    let m = repo
        .alliance_of(viewer)
        .await?
        .ok_or(ForumError::NotAMember)?;
    if announcement && !has_right(m.role, m.rights, AllianceRight::Announce) {
        return Err(ForumError::MissingRight);
    }
    if !valid_thread_title(title) || !valid_body(body) {
        return Err(ForumError::Invalid);
    }
    Ok(repo
        .create_thread(
            m.alliance,
            viewer,
            title.trim(),
            body.trim(),
            announcement,
            now,
        )
        .await?)
}

/// Reply to a thread of the actor's alliance (027 AC3). Rejected if the thread is an announcement (locked).
///
/// # Errors
/// [`ForumError::NotAMember`], [`ForumError::NotFound`], [`ForumError::Locked`],
/// [`ForumError::Invalid`], or a backend error.
pub async fn reply<R>(
    repo: &R,
    viewer: PlayerId,
    thread: u128,
    body: &str,
    now: Timestamp,
) -> Result<u128, ForumError>
where
    R: AllianceRepository,
{
    let m = repo
        .alliance_of(viewer)
        .await?
        .ok_or(ForumError::NotAMember)?;
    let head = repo
        .thread_head(thread)
        .await?
        .ok_or(ForumError::NotFound)?;
    if head.alliance != m.alliance {
        return Err(ForumError::NotFound);
    }
    if head.announcement {
        return Err(ForumError::Locked);
    }
    if !valid_body(body) {
        return Err(ForumError::Invalid);
    }
    Ok(repo.add_post(thread, viewer, body.trim(), now).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{
        AlliedVillage, DiplomacyEntry, IncomingAttack, Membership, OutgoingInvite, PendingInvite,
        RosterEntry,
    };
    use async_trait::async_trait;
    use eperica_domain::{
        AllianceId, AllianceRole, DiplomacyStance, DiplomacyStatus, RightSet, VillageId,
    };
    use std::sync::Mutex;

    #[derive(Clone)]
    struct ThreadRow {
        id: u128,
        alliance: AllianceId,
        title: String,
        announcement: bool,
    }

    /// Minimal in-memory `AllianceRepository` for forum use-case tests: members + threads + posts. All
    /// non-forum methods are inert stubs (unused by these tests).
    #[derive(Default)]
    struct FakeForum {
        members: Mutex<Vec<(u128, Membership)>>,
        threads: Mutex<Vec<ThreadRow>>,
        posts: Mutex<Vec<(u128, String)>>, // (thread_id, body)
        next: Mutex<u128>,
    }

    impl FakeForum {
        fn join(&self, player: u128, alliance: u128, role: AllianceRole, rights: RightSet) {
            self.members.lock().unwrap().push((
                player,
                Membership {
                    alliance: AllianceId(alliance),
                    role,
                    rights,
                },
            ));
        }
    }

    #[async_trait]
    impl AllianceRepository for FakeForum {
        async fn alliance_of(&self, player: PlayerId) -> Result<Option<Membership>, RepoError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .iter()
                .find(|(p, _)| *p == player.0)
                .map(|(_, m)| m.clone()))
        }
        async fn create_thread(
            &self,
            alliance: AllianceId,
            _author: PlayerId,
            title: &str,
            body: &str,
            announcement: bool,
            _now: Timestamp,
        ) -> Result<u128, RepoError> {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            let id = *n;
            self.threads.lock().unwrap().push(ThreadRow {
                id,
                alliance,
                title: title.to_owned(),
                announcement,
            });
            self.posts.lock().unwrap().push((id, body.to_owned()));
            Ok(id)
        }
        async fn list_threads(
            &self,
            alliance: AllianceId,
            _limit: i64,
        ) -> Result<Vec<ThreadSummary>, RepoError> {
            Ok(self
                .threads
                .lock()
                .unwrap()
                .iter()
                .filter(|t| t.alliance == alliance)
                .map(|t| ThreadSummary {
                    id: t.id,
                    title: t.title.clone(),
                    author_name: "a".to_owned(),
                    announcement: t.announcement,
                    post_count: 1,
                    last_post_ms: 0,
                })
                .collect())
        }
        async fn thread_head(&self, thread: u128) -> Result<Option<ThreadHead>, RepoError> {
            Ok(self
                .threads
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.id == thread)
                .map(|t| ThreadHead {
                    alliance: t.alliance,
                    title: t.title.clone(),
                    announcement: t.announcement,
                }))
        }
        async fn add_post(
            &self,
            thread: u128,
            _author: PlayerId,
            body: &str,
            _now: Timestamp,
        ) -> Result<u128, RepoError> {
            self.posts.lock().unwrap().push((thread, body.to_owned()));
            Ok(1)
        }
        async fn list_posts(&self, thread: u128, _limit: i64) -> Result<Vec<ForumPost>, RepoError> {
            Ok(self
                .posts
                .lock()
                .unwrap()
                .iter()
                .filter(|(t, _)| *t == thread)
                .map(|(_, b)| ForumPost {
                    author_name: "a".to_owned(),
                    body: b.clone(),
                    created_ms: 0,
                })
                .collect())
        }

        // ---- inert stubs (unused here) ----
        async fn max_embassy_level(&self, _p: PlayerId) -> Result<u8, RepoError> {
            Ok(0)
        }
        async fn member_count(&self, _a: AllianceId) -> Result<u32, RepoError> {
            Ok(0)
        }
        async fn alliance_summary(
            &self,
            _a: AllianceId,
        ) -> Result<Option<(String, String)>, RepoError> {
            Ok(None)
        }
        async fn roster(&self, _a: AllianceId) -> Result<Vec<RosterEntry>, RepoError> {
            Ok(vec![])
        }
        async fn create_alliance(
            &self,
            _n: &str,
            _t: &str,
            _f: PlayerId,
        ) -> Result<AllianceId, RepoError> {
            Ok(AllianceId(0))
        }
        async fn insert_invite(&self, _a: AllianceId, _p: PlayerId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn delete_invite(&self, _a: AllianceId, _p: PlayerId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn has_invite(&self, _a: AllianceId, _p: PlayerId) -> Result<bool, RepoError> {
            Ok(false)
        }
        async fn pending_invites_for(&self, _p: PlayerId) -> Result<Vec<PendingInvite>, RepoError> {
            Ok(vec![])
        }
        async fn invites_of(&self, _a: AllianceId) -> Result<Vec<OutgoingInvite>, RepoError> {
            Ok(vec![])
        }
        async fn add_member(
            &self,
            _a: AllianceId,
            _p: PlayerId,
            _r: AllianceRole,
            _rt: RightSet,
            _cap: u32,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn remove_member(&self, _p: PlayerId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn set_member_role(
            &self,
            _a: AllianceId,
            _p: PlayerId,
            _r: AllianceRole,
            _rt: RightSet,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn transfer_founder(
            &self,
            _a: AllianceId,
            _from: PlayerId,
            _to: PlayerId,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn disband(&self, _a: AllianceId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn diplomacy_state(
            &self,
            _a: AllianceId,
            _b: AllianceId,
        ) -> Result<Option<(DiplomacyStance, DiplomacyStatus, Option<AllianceId>)>, RepoError>
        {
            Ok(None)
        }
        async fn set_diplomacy_state(
            &self,
            _a: AllianceId,
            _b: AllianceId,
            _stance: DiplomacyStance,
            _status: DiplomacyStatus,
            _by: Option<AllianceId>,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn clear_diplomacy(&self, _a: AllianceId, _b: AllianceId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn diplomacy_of(&self, _a: AllianceId) -> Result<Vec<DiplomacyEntry>, RepoError> {
            Ok(vec![])
        }
        async fn confederate_alliances(
            &self,
            _a: AllianceId,
        ) -> Result<Vec<AllianceId>, RepoError> {
            Ok(vec![])
        }
        async fn alliance_member_villages(
            &self,
            _a: &[AllianceId],
        ) -> Result<Vec<AlliedVillage>, RepoError> {
            Ok(vec![])
        }
        async fn incoming_against(
            &self,
            _v: &[VillageId],
        ) -> Result<Vec<IncomingAttack>, RepoError> {
            Ok(vec![])
        }
    }

    fn member_repo() -> FakeForum {
        let f = FakeForum::default();
        // player 1: founder of alliance 10 (holds all rights incl. Announce).
        f.join(1, 10, AllianceRole::Founder, RightSet::all());
        // player 2: plain member of alliance 10 (no rights).
        f.join(2, 10, AllianceRole::Member, RightSet::empty());
        // player 3: founder of a different alliance 20.
        f.join(3, 20, AllianceRole::Founder, RightSet::all());
        f
    }

    #[tokio::test]
    async fn non_member_is_refused() {
        let f = FakeForum::default();
        assert_eq!(
            list_forum(&f, PlayerId(9)).await.unwrap_err(),
            ForumError::NotAMember
        );
        assert_eq!(
            start_thread(&f, PlayerId(9), "t", "b", false, Timestamp(0))
                .await
                .unwrap_err(),
            ForumError::NotAMember
        );
    }

    #[tokio::test]
    async fn announcement_requires_the_announce_right() {
        let f = member_repo();
        // Plain member cannot start an announcement.
        assert_eq!(
            start_thread(&f, PlayerId(2), "Notice", "hi", true, Timestamp(0))
                .await
                .unwrap_err(),
            ForumError::MissingRight
        );
        // Founder (has Announce) can; a plain member can still start an ordinary thread.
        assert!(
            start_thread(&f, PlayerId(1), "Notice", "hi", true, Timestamp(0))
                .await
                .is_ok()
        );
        assert!(
            start_thread(&f, PlayerId(2), "Chat", "hi", false, Timestamp(0))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn reply_to_locked_thread_is_rejected() {
        let f = member_repo();
        let ann = start_thread(&f, PlayerId(1), "Rules", "read", true, Timestamp(0))
            .await
            .unwrap();
        assert_eq!(
            reply(&f, PlayerId(2), ann, "me too", Timestamp(0))
                .await
                .unwrap_err(),
            ForumError::Locked
        );
        let chat = start_thread(&f, PlayerId(1), "Chat", "hello", false, Timestamp(0))
            .await
            .unwrap();
        assert!(
            reply(&f, PlayerId(2), chat, "hi", Timestamp(0))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn cross_alliance_access_is_not_found() {
        let f = member_repo();
        let t = start_thread(&f, PlayerId(1), "Secret", "plans", false, Timestamp(0))
            .await
            .unwrap();
        // Player 3 (alliance 20) cannot open alliance 10's thread.
        assert_eq!(
            open_thread(&f, PlayerId(3), t).await.unwrap_err(),
            ForumError::NotFound
        );
        assert_eq!(
            reply(&f, PlayerId(3), t, "intruder", Timestamp(0))
                .await
                .unwrap_err(),
            ForumError::NotFound
        );
        // A member of alliance 10 can.
        assert!(open_thread(&f, PlayerId(2), t).await.is_ok());
    }

    #[tokio::test]
    async fn invalid_title_or_body_rejected() {
        let f = member_repo();
        assert_eq!(
            start_thread(&f, PlayerId(1), "  ", "body", false, Timestamp(0))
                .await
                .unwrap_err(),
            ForumError::Invalid
        );
        assert_eq!(
            start_thread(&f, PlayerId(1), "Title", "  ", false, Timestamp(0))
                .await
                .unwrap_err(),
            ForumError::Invalid
        );
    }
}
