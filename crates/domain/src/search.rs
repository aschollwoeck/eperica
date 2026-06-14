//! Search query parsing (028). Pure (P3) — no I/O.
//!
//! The only rule here is recognising a **coordinate** query so the who-is search can offer a direct map
//! jump. Name/tag matching is a persistence concern (a bounded prefix scan), not a domain rule.

use crate::world::Coordinate;

/// Parse a search query as a map coordinate, accepting `x|y`, `(x|y)`, `x,y`, or `x y` with optional
/// surrounding whitespace and parentheses (028 AC3). Returns `None` if it is not a coordinate.
pub fn parse_coordinate(query: &str) -> Option<Coordinate> {
    let q = query.trim().trim_start_matches('(').trim_end_matches(')');
    // Try each separator in turn; coordinates use `|` canonically, but be forgiving.
    let (xs, ys) = q
        .split_once('|')
        .or_else(|| q.split_once(','))
        .or_else(|| q.split_once(char::is_whitespace))?;
    let x: i32 = xs.trim().parse().ok()?;
    let y: i32 = ys.trim().parse().ok()?;
    Some(Coordinate::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_accepted_form() {
        let want = Coordinate::new(12, -7);
        assert_eq!(parse_coordinate("12|-7"), Some(want));
        assert_eq!(parse_coordinate("(12|-7)"), Some(want));
        assert_eq!(parse_coordinate("12,-7"), Some(want));
        assert_eq!(parse_coordinate("12 -7"), Some(want));
        assert_eq!(parse_coordinate("  ( 12 | -7 )  "), Some(want));
        assert_eq!(parse_coordinate("0|0"), Some(Coordinate::new(0, 0)));
    }

    #[test]
    fn rejects_non_coordinates() {
        assert_eq!(parse_coordinate(""), None);
        assert_eq!(parse_coordinate("alice"), None);
        assert_eq!(parse_coordinate("12|"), None);
        assert_eq!(parse_coordinate("|7"), None);
        assert_eq!(parse_coordinate("12|x"), None);
        assert_eq!(parse_coordinate("1|2|3"), None);
    }
}
