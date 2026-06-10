//! Password hashing — the argon2 adapter for the application's [`PasswordHasher`] port.

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString};
use eperica_application::{PasswordHasher, RepoError};

/// Argon2id password hasher (default parameters).
#[derive(Debug, Default, Clone, Copy)]
pub struct Argon2Hasher;

impl PasswordHasher for Argon2Hasher {
    fn hash(&self, password: &str) -> Result<String, RepoError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| RepoError::Backend(format!("password hashing failed: {e}")))?;
        Ok(hash.to_string())
    }

    fn verify(&self, password: &str, hash: &str) -> Result<bool, RepoError> {
        let parsed = PasswordHash::new(hash)
            .map_err(|e| RepoError::Backend(format!("stored hash is invalid: {e}")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies() {
        let h = Argon2Hasher;
        let hash = h.hash("correct horse").unwrap();
        assert!(h.verify("correct horse", &hash).unwrap());
        assert!(!h.verify("wrong", &hash).unwrap());
    }
}
