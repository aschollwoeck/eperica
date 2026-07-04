//! Bot key manifest loading and validation (slice 120 format).
//!
//! The key manifest is the one-time JSON array emitted by the admin "Seed
//! bots" action.  It has the shape `[{"username":"…","token":"epk_…"},…]`.

use crate::client::{ApiClient, ApiFailure};
use crate::digest::MeResponse;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One entry in the key manifest produced by slice 120.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestEntry {
    /// The bot's username (as registered in Eperica).
    pub username: String,
    /// Bearer token in `epk_<id>_<secret>` format.
    pub token: String,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Parse a manifest JSON string into a list of entries.
///
/// Returns an error string if the JSON is malformed or any entry is missing a
/// required field (`username` or `token`).
fn parse_manifest(s: &str) -> Result<Vec<ManifestEntry>, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// Read and parse the key manifest at `path`.
///
/// Returns an error string on I/O failure or JSON parse failure.
pub fn load_manifest(path: &str) -> Result<Vec<ManifestEntry>, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_manifest(&contents)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Call `/api/me` for each entry and return the outcome alongside the entry.
///
/// Live entries return `Ok(MeResponse)`; dead or revoked keys return
/// `Err(ApiFailure)`.  The runner (T4) promotes live entries to `Bot` and
/// logs or discards dead ones without failing the fleet.
pub async fn validate(
    base_url: &str,
    entries: Vec<ManifestEntry>,
) -> Vec<(ManifestEntry, Result<MeResponse, ApiFailure>)> {
    let mut results = Vec::with_capacity(entries.len());
    for entry in entries {
        let client = ApiClient::new(base_url, &entry.token);
        let outcome = client.me().await;
        results.push((entry, outcome));
    }
    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_manifest() {
        let json = r#"[{"username": "bot_01", "token": "epk_aabbcc_deadbeef"}]"#;
        let entries = parse_manifest(json).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].username, "bot_01");
        assert_eq!(entries[0].token, "epk_aabbcc_deadbeef");
    }

    #[test]
    fn multiple_entries() {
        let json = r#"[
            {"username": "alpha", "token": "epk_1_aaa"},
            {"username": "beta",  "token": "epk_2_bbb"}
        ]"#;
        let entries = parse_manifest(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].username, "beta");
    }

    #[test]
    fn empty_manifest() {
        let entries = parse_manifest("[]").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn bad_json() {
        let err = parse_manifest("not json at all").unwrap_err();
        assert!(!err.is_empty(), "error message should be non-empty");
    }

    #[test]
    fn missing_token_field() {
        // token is required — missing it should fail deserialization.
        let json = r#"[{"username": "bot_01"}]"#;
        assert!(
            parse_manifest(json).is_err(),
            "missing `token` field must be an error"
        );
    }

    #[test]
    fn missing_username_field() {
        // username is required — missing it should fail deserialization.
        let json = r#"[{"token": "epk_xyz_abc"}]"#;
        assert!(
            parse_manifest(json).is_err(),
            "missing `username` field must be an error"
        );
    }
}
