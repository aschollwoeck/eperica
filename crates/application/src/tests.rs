//! Use-case tests against in-memory fakes (no I/O) — covers AC1 (register) and AC2 (login).

use crate::auth::{LoginError, authenticate};
use crate::ports::{AccountRepository, NewUser, PasswordHasher, RepoError, UserRecord};
use crate::register::{RegisterCommand, RegisterError, register};
use async_trait::async_trait;
use eperica_domain::{
    BuildingKind, BuildingSlot, PlayerId, ResourceAmounts, ResourceField, ResourceKind,
    StartingVillage, Timestamp, Tribe, UnitCounts, Village, VillageId,
};
use std::sync::Mutex;

fn template() -> StartingVillage {
    let mut fields = Vec::new();
    for kind in [ResourceKind::Wood, ResourceKind::Clay, ResourceKind::Iron] {
        fields.extend(std::iter::repeat_n(ResourceField { kind, level: 0 }, 4));
    }
    fields.extend(std::iter::repeat_n(
        ResourceField {
            kind: ResourceKind::Crop,
            level: 0,
        },
        6,
    ));
    StartingVillage::new(
        fields,
        vec![
            BuildingSlot {
                kind: BuildingKind::MainBuilding,
                level: 1,
            },
            BuildingSlot {
                kind: BuildingKind::RallyPoint,
                level: 1,
            },
        ],
    )
    .expect("valid template")
}

#[derive(Default)]
struct InMemoryAccounts {
    users: Mutex<Vec<UserRecord>>,
}

#[async_trait]
impl AccountRepository for InMemoryAccounts {
    async fn create_account(
        &self,
        user: NewUser,
        _template: &StartingVillage,
    ) -> Result<UserRecord, RepoError> {
        let mut users = self.users.lock().unwrap();
        if users
            .iter()
            .any(|u| u.username == user.username || u.email == user.email)
        {
            return Err(RepoError::Duplicate);
        }
        let rec = UserRecord {
            id: PlayerId(users.len() as u128 + 1),
            username: user.username,
            email: user.email,
            password_hash: user.password_hash,
            email_confirmed: user.email_confirmed,
            tribe: user.tribe,
            abandoned: false,
        };
        users.push(rec.clone());
        Ok(rec)
    }

    async fn find_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, RepoError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.username == username)
            .cloned())
    }

    async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.id == id)
            .cloned())
    }

    async fn villages_of(&self, _owner: PlayerId) -> Result<Vec<Village>, RepoError> {
        Ok(Vec::new())
    }

    async fn village_by_id(&self, _village: VillageId) -> Result<Option<Village>, RepoError> {
        Ok(None)
    }

    async fn stored_resources(
        &self,
        _village: VillageId,
    ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
        Ok(None)
    }

    async fn garrison(&self, _village: VillageId) -> Result<UnitCounts, RepoError> {
        Ok(Vec::new())
    }

    async fn villages_at(
        &self,
        _coords: &[eperica_domain::Coordinate],
    ) -> Result<Vec<crate::ports::VillageMarker>, RepoError> {
        Ok(Vec::new())
    }

    async fn village_at(
        &self,
        _coord: eperica_domain::Coordinate,
    ) -> Result<Option<Village>, RepoError> {
        Ok(None)
    }
}

struct FakeHasher;
impl PasswordHasher for FakeHasher {
    fn hash(&self, password: &str) -> Result<String, RepoError> {
        Ok(format!("hashed:{password}"))
    }
    fn verify(&self, password: &str, hash: &str) -> Result<bool, RepoError> {
        Ok(hash == format!("hashed:{password}"))
    }
}

fn cmd(name: &str) -> RegisterCommand {
    RegisterCommand {
        username: name.to_owned(),
        email: format!("{name}@example.com"),
        password: "secret123".to_owned(),
        tribe: "gauls".to_owned(),
    }
}

#[tokio::test]
async fn register_creates_account() {
    let accounts = InMemoryAccounts::default();
    let user = register(&accounts, &FakeHasher, &template(), false, cmd("alice"))
        .await
        .unwrap();
    assert_eq!(user.username, "alice");
    assert!(user.email_confirmed); // confirmation not required
    assert_eq!(user.password_hash, "hashed:secret123");
    assert_eq!(user.tribe, Tribe::Gauls); // 004 AC1: the chosen tribe is stored
}

#[tokio::test]
async fn register_stores_each_chosen_tribe() {
    // 004 AC1: every valid tribe choice is persisted as chosen.
    let accounts = InMemoryAccounts::default();
    for (name, slug, tribe) in [
        ("rome", "romans", Tribe::Romans),
        ("teut", "teutons", Tribe::Teutons),
        ("gaul", "gauls", Tribe::Gauls),
    ] {
        let mut c = cmd(name);
        c.tribe = slug.to_owned();
        let user = register(&accounts, &FakeHasher, &template(), false, c)
            .await
            .unwrap();
        assert_eq!(user.tribe, tribe);
    }
}

#[tokio::test]
async fn register_rejects_missing_or_unknown_tribe() {
    // 004 AC1: a registration without a valid tribe is rejected server-side.
    let accounts = InMemoryAccounts::default();
    for bad in ["", "  ", "egyptians"] {
        let mut c = cmd("tribetest");
        c.tribe = bad.to_owned();
        assert!(matches!(
            register(&accounts, &FakeHasher, &template(), false, c).await,
            Err(RegisterError::Invalid(_))
        ));
    }
    assert!(accounts.users.lock().unwrap().is_empty());
}

#[tokio::test]
async fn register_rejects_duplicate() {
    let accounts = InMemoryAccounts::default();
    register(&accounts, &FakeHasher, &template(), false, cmd("bob"))
        .await
        .unwrap();
    let err = register(&accounts, &FakeHasher, &template(), false, cmd("bob"))
        .await
        .unwrap_err();
    assert_eq!(err, RegisterError::Taken);
}

#[tokio::test]
async fn register_rejects_invalid_input() {
    let accounts = InMemoryAccounts::default();

    let mut blank = cmd("ignored");
    blank.username = "   ".to_owned();
    assert!(matches!(
        register(&accounts, &FakeHasher, &template(), false, blank).await,
        Err(RegisterError::Invalid(_))
    ));

    let mut bad_email = cmd("emailtest");
    bad_email.email = "notanemail".to_owned();
    assert!(matches!(
        register(&accounts, &FakeHasher, &template(), false, bad_email).await,
        Err(RegisterError::Invalid(_))
    ));

    let mut short_pw = cmd("pwtest");
    short_pw.password = "short".to_owned();
    assert!(matches!(
        register(&accounts, &FakeHasher, &template(), false, short_pw).await,
        Err(RegisterError::Invalid(_))
    ));

    // No account was created by any rejected attempt.
    assert!(
        accounts
            .find_user_by_username("emailtest")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn register_requires_confirmation_when_enabled() {
    let accounts = InMemoryAccounts::default();
    let user = register(&accounts, &FakeHasher, &template(), true, cmd("carol"))
        .await
        .unwrap();
    assert!(!user.email_confirmed);
}

#[tokio::test]
async fn login_succeeds_with_correct_password() {
    let accounts = InMemoryAccounts::default();
    register(&accounts, &FakeHasher, &template(), false, cmd("dave"))
        .await
        .unwrap();
    let user = authenticate(&accounts, &FakeHasher, "dave", "secret123")
        .await
        .unwrap();
    assert_eq!(user.username, "dave");
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let accounts = InMemoryAccounts::default();
    register(&accounts, &FakeHasher, &template(), false, cmd("erin"))
        .await
        .unwrap();
    let err = authenticate(&accounts, &FakeHasher, "erin", "wrong")
        .await
        .unwrap_err();
    assert_eq!(err, LoginError::InvalidCredentials);
}

#[tokio::test]
async fn login_rejects_unknown_user() {
    let accounts = InMemoryAccounts::default();
    let err = authenticate(&accounts, &FakeHasher, "ghost", "secret123")
        .await
        .unwrap_err();
    assert_eq!(err, LoginError::InvalidCredentials);
}

#[tokio::test]
async fn login_rejects_unconfirmed_email() {
    let accounts = InMemoryAccounts::default();
    register(&accounts, &FakeHasher, &template(), true, cmd("frank"))
        .await
        .unwrap();
    let err = authenticate(&accounts, &FakeHasher, "frank", "secret123")
        .await
        .unwrap_err();
    assert_eq!(err, LoginError::EmailNotConfirmed);
}
