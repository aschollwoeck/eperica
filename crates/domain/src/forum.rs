//! Alliance-forum validation (027). Pure (P3) — no I/O.
//!
//! Thread titles are validated here; post bodies reuse the 024 chat rules ([`crate::comms::valid_body`]).

/// Max length (characters) of a thread title.
pub const MAX_THREAD_TITLE: usize = 120;

/// Whether a thread `title` is valid (027 AC6): non-empty after trimming and within [`MAX_THREAD_TITLE`]
/// characters. Rendered as plain text — no markup interpretation.
pub fn valid_thread_title(title: &str) -> bool {
    let t = title.trim();
    !t.is_empty() && t.chars().count() <= MAX_THREAD_TITLE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_bounds() {
        assert!(!valid_thread_title(""));
        assert!(!valid_thread_title("   "));
        assert!(valid_thread_title("War plans for the eastern front"));
        assert!(valid_thread_title(&"x".repeat(MAX_THREAD_TITLE)));
        assert!(!valid_thread_title(&"x".repeat(MAX_THREAD_TITLE + 1)));
        // Trimmed length is what counts.
        assert!(valid_thread_title(&format!(
            "  {}  ",
            "x".repeat(MAX_THREAD_TITLE)
        )));
    }
}
