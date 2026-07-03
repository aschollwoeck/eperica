//! Agent API key utilities (118).
//!
//! ## Format
//!
//! `epk_<id>_<secret>` where:
//! - `id` is 16 lowercase hex chars (8 random bytes) — the indexed lookup half.
//! - `secret` is 43 base64url chars (no padding, 32 random bytes) — the secret half.
//!
//! ## Storage (Decision #2, P11)
//!
//! Only `sha256(secret)` (hex) is persisted. A 256-bit random secret gains nothing from argon2
//! stretching — the entropy is already at the ceiling — and SHA-256 verification adds ~0 overhead
//! on the P11 hot path (every agent request). A DB leak still reveals no usable keys.
//! Keys are revocable via `revoked_at`; the plaintext is returned exactly once at creation.

use base64::Engine as _;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// The two public components of a generated agent key.
///
/// Returned by [`generate`] alongside the full plaintext bearer token.
#[derive(Debug, Clone)]
pub struct AgentKey {
    /// 16 lowercase hex chars (8 random bytes) — the indexed lookup half, safe to log.
    pub id: String,
    /// 43 base64url chars (32 random bytes, no padding) — the secret half, **never log**.
    pub secret: String,
}

/// Generate a fresh key pair.
///
/// Returns the [`AgentKey`] parts and the full plaintext token `epk_<id>_<secret>` (shown exactly
/// once at creation; only [`secret_hash`] is stored — Decision #2, P11).
pub fn generate() -> (AgentKey, String) {
    use rand::RngCore as _;
    let mut rng = rand::thread_rng();

    let mut id_bytes = [0u8; 8];
    rng.fill_bytes(&mut id_bytes);
    let id = bytes_to_hex(&id_bytes);

    let mut secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut secret_bytes);
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret_bytes);

    let token = format!("epk_{id}_{secret}");
    (AgentKey { id, secret }, token)
}

/// Parse a bearer token into `(id, secret)`.
///
/// Returns `None` when the format is invalid — wrong prefix, wrong component count, or wrong
/// lengths. Strict format validation ensures malformed tokens are rejected before any DB lookup.
pub fn parse(token: &str) -> Option<(String, String)> {
    let rest = token.strip_prefix("epk_")?;
    // Split on the first `_` only: id cannot contain `_`, secret may not contain `_` but we
    // enforce length anyway. Using splitn(2) so the secret is not further split.
    let (id, secret) = rest.split_once('_')?;

    // id: exactly 16 lowercase hex chars (8 bytes) — uppercase is rejected (generation is lowercase).
    if id.len() != 16
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    // secret: exactly 43 base64url chars (32 bytes, no padding), charset-checked.
    if secret.len() != 43
        || !secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    Some((id.to_owned(), secret.to_owned()))
}

/// SHA-256 hash of `secret`, as 64 lowercase hex chars (Decision #2 — the persisted value).
pub fn secret_hash(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    bytes_to_hex(&digest)
}

/// Constant-time comparison of a presented `secret` against a `stored_hash` (SHA-256 hex).
///
/// Returns `true` iff `sha256(secret) == stored_hash`. Constant-time prevents timing attacks on
/// key lookup (P11).
pub fn verify(secret: &str, stored_hash: &str) -> bool {
    let presented = secret_hash(secret);
    // Both hex strings are the same length (64 bytes); ConstantTimeEq is safe.
    presented.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}

/// Encode `bytes` as lowercase hex.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            use std::fmt::Write as _;
            write!(s, "{b:02x}").expect("write to String is infallible");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A generated token round-trips through parse: the id and secret components match.
    #[test]
    fn generate_parse_round_trip() {
        let (key, token) = generate();
        let (id, secret) = parse(&token).expect("generated token must parse");
        assert_eq!(id, key.id, "id half matches");
        assert_eq!(secret, key.secret, "secret half matches");
    }

    /// The generated id is 16 hex chars; the secret is 43 base64url chars.
    #[test]
    fn generated_lengths() {
        let (key, _token) = generate();
        assert_eq!(key.id.len(), 16, "id is 16 hex chars");
        assert_eq!(key.secret.len(), 43, "secret is 43 base64url chars");
        assert!(
            key.id.bytes().all(|b| b.is_ascii_hexdigit()),
            "id is lowercase hex"
        );
    }

    /// parse rejects a wrong prefix.
    #[test]
    fn parse_rejects_wrong_prefix() {
        assert!(
            parse("apk_0123456789abcdef_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none()
        );
        assert!(parse("epk_").is_none());
        assert!(parse("").is_none());
        assert!(parse("0123456789abcdef_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
    }

    /// parse rejects wrong component count (too few or extra `_` in unexpected places).
    #[test]
    fn parse_rejects_wrong_part_count() {
        // No secret part at all.
        assert!(parse("epk_0123456789abcdef").is_none());
    }

    /// parse rejects an id that is the wrong length.
    #[test]
    fn parse_rejects_wrong_id_length() {
        // id too short (15 chars).
        assert!(
            parse("epk_0123456789abcde_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none()
        );
        // id too long (17 chars).
        assert!(
            parse("epk_0123456789abcdefg_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none()
        );
    }

    /// parse rejects a secret that is the wrong length.
    #[test]
    fn parse_rejects_wrong_secret_length() {
        // secret too short (42 chars).
        assert!(parse("epk_0123456789abcdef_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
        // secret too long (44 chars).
        assert!(
            parse("epk_0123456789abcdef_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none()
        );
    }

    /// verify returns true for the correct secret and false for any other.
    #[test]
    fn verify_correct_and_wrong() {
        let (_key, token) = generate();
        let (_id, secret) = parse(&token).unwrap();
        let hash = secret_hash(&secret);

        assert!(verify(&secret, &hash), "correct secret verifies");
        assert!(!verify("wrong_secret", &hash), "wrong secret is rejected");
        assert!(!verify("", &hash), "empty secret is rejected");
    }

    /// secret_hash produces a 64-char lowercase hex string.
    #[test]
    fn hash_is_64_hex_chars() {
        let hash = secret_hash("some_secret");
        assert_eq!(hash.len(), 64, "SHA-256 hex is 64 chars");
        assert!(
            hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "all chars are hex digits"
        );
    }
}
