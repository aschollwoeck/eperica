//! Strategy overlay: LLM-derived goal knobs that bias the pure reflex doctrine.
//!
//! `Strategy` is data — not a second brain. `plan_tick` reads it alongside the
//! `Persona` to apply directional biases; the decision logic itself stays pure.
//!
//! The strict no-fallback rule (from the 121 operator directive) applies here:
//! invalid strategist output is rejected loudly and the prior `Strategy` persists —
//! values are never silently clamped or guessed.
//!
//! # Quadrant convention
//!
//! Used throughout this module and by `policy::plan_tick`:
//! - positive x = east; negative y = north (map rows run north→south, so a
//!   smaller y coordinate is further north).
//! - NE: dx > 0, dy < 0 — east of and north of the acting village.
//! - NW: dx < 0, dy < 0 — west of and north of the acting village.
//! - SE: dx > 0, dy > 0 — east of and south of the acting village.
//! - SW: dx < 0, dy > 0 — west of and south of the acting village.
//! - Axis-zero tiles (dx == 0 or dy == 0) count as a non-match for every quadrant.

use crate::digest::{Digest, MapWindow};
use crate::persona::Persona;
use crate::policy::BotTribe;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Strategic focus: a directional bias applied on top of the base reflex doctrine.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Focus {
    /// No bias — byte-identical to 121 behaviour (the default).
    #[default]
    Balanced,
    /// Prioritise economic infrastructure; suppress raiding unless the garrison
    /// is at least twice the normal raid floor.
    Economy,
    /// Raise the training floor (×2) and expand the raid party cap (+4).
    Military,
    /// Prioritise the settler chain (Residence→10 jumps the doctrine queue)
    /// once a Residence exists at any level.
    Expansion,
}

/// Cardinal quadrant on the game map.
///
/// See the module-level quadrant convention for the coordinate mapping.
#[derive(Debug, Clone, PartialEq)]
pub enum Quadrant {
    NE,
    NW,
    SE,
    SW,
}

/// LLM-derived strategy overlay for one bot.
///
/// `Strategy::default()` biases nothing — `plan_tick` with a default `Strategy`
/// produces byte-identical intents to the 121 baseline (AC1 invariant).
#[derive(Debug, Clone, PartialEq)]
pub struct Strategy {
    /// Directional bias applied to the doctrine.
    pub focus: Focus,
    /// Aggression level override (0–3). `None` ⇒ use the persona's value.
    pub aggression: Option<u8>,
    /// Preferred quadrant for raid-target ordering. `None` ⇒ distance-only.
    pub raid_quadrant: Option<Quadrant>,
    /// Preferred quadrant for settling-valley ordering. `None` ⇒ distance-only.
    pub settle_quadrant: Option<Quadrant>,
    /// Free-text log label (used in structured logs only; no gameplay effect).
    pub motto: String,
}

impl Default for Strategy {
    fn default() -> Self {
        Self {
            focus: Focus::Balanced,
            aggression: None,
            raid_quadrant: None,
            settle_quadrant: None,
            motto: String::new(),
        }
    }
}

/// An outbound diplomatic message to be sent via the 119 message endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboundMessage {
    /// Recipient username.
    pub to: String,
    /// Message body (1–500 chars).
    pub body: String,
}

/// Parsed output from the strategist LLM.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategistReply {
    pub strategy: Strategy,
    /// Optional outbound diplomatic message (at most one per cycle).
    pub message: Option<OutboundMessage>,
}

// ---------------------------------------------------------------------------
// Quadrant helper (pub(crate) — used by policy::plan_tick for sorting)
// ---------------------------------------------------------------------------

/// Return `true` when the cell at offset `(dx, dy)` from the acting village
/// falls in `q`.  Axis-zero offsets (dx == 0 or dy == 0) are never a match.
pub(crate) fn in_quadrant(dx: i32, dy: i32, q: &Quadrant) -> bool {
    if dx == 0 || dy == 0 {
        return false;
    }
    matches!(
        (dx.signum(), dy.signum(), q),
        (1, -1, Quadrant::NE)
            | (-1, -1, Quadrant::NW)
            | (1, 1, Quadrant::SE)
            | (-1, 1, Quadrant::SW)
    )
}

// ---------------------------------------------------------------------------
// Intermediate serde DTOs — strict, deny_unknown_fields
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrategyDto {
    focus: String,
    #[serde(default)]
    aggression: Option<u8>,
    #[serde(default)]
    raid_quadrant: Option<String>,
    #[serde(default)]
    settle_quadrant: Option<String>,
    motto: String,
    #[serde(default)]
    message: Option<MessageDto>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageDto {
    to: String,
    body: String,
}

// ---------------------------------------------------------------------------
// parse_reply
// ---------------------------------------------------------------------------

/// Parse a raw LLM reply into a [`StrategistReply`].
///
/// # Strict contract (the 121 no-fallback rule)
///
/// The input must be EXACTLY a JSON object after stripping leading/trailing
/// whitespace — a reply wrapped in prose or ` ```json ` fences is an `Err`.
/// Strictness is intentional: the prompt demands bare JSON; any deviation
/// signals a prompt-contract violation that must be logged loudly, never
/// silently patched up.
///
/// - Unknown fields → `Err` (serde `deny_unknown_fields`).
/// - `focus` must be `"balanced"`, `"economy"`, `"military"`, or `"expansion"`.
/// - `aggression` when present must be ≤ 3; values > 3 are `Err`, never clamped.
/// - `raid_quadrant` / `settle_quadrant` when present: `"ne"`, `"nw"`, `"se"`, or `"sw"`.
/// - `motto` length (Unicode code points) must be ≤ 80.
/// - `message.to` must be non-empty; `message.body` must be 1–500 code points.
pub fn parse_reply(raw: &str) -> Result<StrategistReply, String> {
    let trimmed = raw.trim();

    // Strict bare-object check: must start with '{' and end with '}'.
    // This catches prose-wrapped replies and ```json fence wrappers without a
    // two-pass parse.  A reply starting with '{' and ending with '}' could
    // still fail serde parsing (malformed JSON) which is also an Err.
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(format!(
            "reply is not a bare JSON object (first char={:?}, last char={:?}); \
             the prompt demands ONLY a JSON object — no prose, no fences",
            trimmed.chars().next(),
            trimmed.chars().last(),
        ));
    }

    let dto: StrategyDto =
        serde_json::from_str(trimmed).map_err(|e| format!("JSON parse/schema error: {e}"))?;

    // Validate focus.
    let focus = match dto.focus.as_str() {
        "balanced" => Focus::Balanced,
        "economy" => Focus::Economy,
        "military" => Focus::Military,
        "expansion" => Focus::Expansion,
        other => {
            return Err(format!(
                "unknown focus {other:?}; expected balanced|economy|military|expansion"
            ));
        }
    };

    // Validate aggression range (no clamping — out-of-range is an error).
    if let Some(agg) = dto.aggression
        && agg > 3
    {
        return Err(format!(
            "aggression {agg} is out of range (0–3); values are never clamped"
        ));
    }

    // Validate raid_quadrant.
    let raid_quadrant = dto.raid_quadrant.map(|s| parse_quadrant(&s)).transpose()?;

    // Validate settle_quadrant.
    let settle_quadrant = dto
        .settle_quadrant
        .map(|s| parse_quadrant(&s))
        .transpose()?;

    // Validate motto length (Unicode code points, not bytes).
    let motto_chars = dto.motto.chars().count();
    if motto_chars > 80 {
        return Err(format!(
            "motto length {motto_chars} exceeds the 80-char cap"
        ));
    }

    // Validate optional message.
    let message = dto
        .message
        .map(|m| {
            if m.to.is_empty() {
                return Err("message.to must be non-empty".to_owned());
            }
            let body_chars = m.body.chars().count();
            if body_chars == 0 || body_chars > 500 {
                return Err(format!(
                    "message.body length {body_chars} is out of range (1–500 chars)"
                ));
            }
            Ok(OutboundMessage {
                to: m.to,
                body: m.body,
            })
        })
        .transpose()?;

    Ok(StrategistReply {
        strategy: Strategy {
            focus,
            aggression: dto.aggression,
            raid_quadrant,
            settle_quadrant,
            motto: dto.motto,
        },
        message,
    })
}

fn parse_quadrant(s: &str) -> Result<Quadrant, String> {
    match s {
        "ne" => Ok(Quadrant::NE),
        "nw" => Ok(Quadrant::NW),
        "se" => Ok(Quadrant::SE),
        "sw" => Ok(Quadrant::SW),
        other => Err(format!("unknown quadrant {other:?}; expected ne|nw|se|sw")),
    }
}

fn quadrant_label(q: &Quadrant) -> &'static str {
    match q {
        Quadrant::NE => "ne",
        Quadrant::NW => "nw",
        Quadrant::SE => "se",
        Quadrant::SW => "sw",
    }
}

// ---------------------------------------------------------------------------
// build_prompt
// ---------------------------------------------------------------------------

/// Assemble a compact, fog-honest strategist prompt.
///
/// # Guarantees
/// - **Pure and deterministic**: identical inputs always produce identical output.
/// - **Bounded by construction**: report list capped at 5, map cells at 10.
/// - **Fog-honest**: only data from the bot's own digest surface is included —
///   no foreign-village troop data, no `reinforcements_here` troops, nothing
///   the bot could not see under the server's fog rules.
pub fn build_prompt(
    d: &Digest,
    map: Option<&MapWindow>,
    p: &Persona,
    current: &Strategy,
    name: &str,
    tribe: BotTribe,
) -> String {
    let tribe_name = match tribe {
        BotTribe::Romans => "Roman",
        BotTribe::Teutons => "Teuton",
        BotTribe::Gauls => "Gaul",
    };

    let focus_str = match &current.focus {
        Focus::Balanced => "balanced",
        Focus::Economy => "economy",
        Focus::Military => "military",
        Focus::Expansion => "expansion",
    };
    let agg_str = current
        .aggression
        .map(|a| a.to_string())
        .unwrap_or_else(|| "none".to_owned());
    let rq_str = current
        .raid_quadrant
        .as_ref()
        .map(quadrant_label)
        .unwrap_or("none");
    let sq_str = current
        .settle_quadrant
        .as_ref()
        .map(quadrant_label)
        .unwrap_or("none");

    // Reference position for map-distance computation: first village, else origin.
    let (ref_x, ref_y) = d.villages.first().map(|v| (v.x, v.y)).unwrap_or((0, 0));

    let mut out = String::with_capacity(2048);

    // --- System-style header (demands bare JSON only) ---
    out.push_str(&format!(
        "You are the strategist for {name}, a {tribe_name} chieftain in a \
         Travian-like war game.\n\
         Reply with ONLY a bare JSON object — no prose, no markdown fences — \
         matching this schema:\n\
         {{\"focus\":\"balanced|economy|military|expansion\",\
         \"aggression\":0-3 (optional),\
         \"raid_quadrant\":\"ne|nw|se|sw\" (optional),\
         \"settle_quadrant\":\"ne|nw|se|sw\" (optional),\
         \"motto\":\"<string max 80 chars>\",\
         \"message\":{{\"to\":\"<username>\",\"body\":\"<string max 500 chars>\"}} \
         (optional)}}\n\n\
         === SITUATION ===\n"
    ));

    // --- Villages ---
    out.push_str(&format!(
        "Villages ({}/{} used):\n",
        d.culture.villages_used, d.culture.villages_allowed
    ));
    for v in &d.villages {
        let garrison_sum: u32 = v.garrison.iter().map(|g| g.count).sum();
        out.push_str(&format!(
            "  [{id}] ({x},{y}): \
             wood={wa}/{wc}(+{wr}/h) clay={ca}/{cc}(+{cr}/h) \
             iron={ia}/{ic}(+{ir}/h) crop={cra}/{crc}(+{crr}/h) \
             | fields:{fc} buildings:{bc} garrison:{garrison}\n",
            id = v.id,
            x = v.x,
            y = v.y,
            wa = v.resources.wood.amount,
            wc = v.resources.wood.capacity,
            wr = v.resources.wood.rate,
            ca = v.resources.clay.amount,
            cc = v.resources.clay.capacity,
            cr = v.resources.clay.rate,
            ia = v.resources.iron.amount,
            ic = v.resources.iron.capacity,
            ir = v.resources.iron.rate,
            cra = v.resources.crop.amount,
            crc = v.resources.crop.capacity,
            crr = v.resources.crop.rate,
            fc = v.fields.len(),
            bc = v.buildings.len(),
            garrison = garrison_sum,
        ));
    }

    // --- Culture ---
    out.push_str(&format!(
        "Culture: {} CP (+{}/h) | {}/{} villages\n",
        d.culture.cp, d.culture.rate_per_hour, d.culture.villages_used, d.culture.villages_allowed,
    ));

    // --- Incoming attacks ---
    out.push_str(&format!("Incoming attacks: {}\n", d.incoming_attacks.len()));

    // --- Recent battle reports (≤5, id omitted per fog rules) ---
    out.push_str("Recent battles (last 5):\n");
    let reports: Vec<_> = d.reports.iter().take(5).collect();
    if reports.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for r in reports {
            out.push_str(&format!(
                "  occurred={} kind={} won={}\n",
                r.occurred_at_ms, r.kind, r.attacker_won,
            ));
        }
    }

    // --- Nearby map cells (≤10, sorted by Chebyshev distance from the first village) ---
    out.push_str("Nearby map (top 10 by distance):\n");
    if let Some(win) = map {
        let mut cells: Vec<_> = win.rows.iter().flatten().collect();
        // Sort deterministically: primary = Chebyshev distance, secondary = (x, y).
        cells.sort_by_key(|c| {
            let dist = (c.x - ref_x)
                .unsigned_abs()
                .max((c.y - ref_y).unsigned_abs());
            (dist, c.x, c.y)
        });
        let shown: Vec<_> = cells.into_iter().take(10).collect();
        if shown.is_empty() {
            out.push_str("  (none)\n");
        } else {
            for c in shown {
                let dist = (c.x - ref_x)
                    .unsigned_abs()
                    .max((c.y - ref_y).unsigned_abs());
                out.push_str(&format!("  {} dist={}\n", c.label, dist));
            }
        }
    } else {
        out.push_str("  (map unavailable)\n");
    }

    // --- Current strategy + persona base aggression ---
    out.push_str(&format!(
        "Current strategy: focus={focus_str} aggression={agg_str} \
         raid_q={rq_str} settle_q={sq_str} motto={motto:?}\n\
         Persona base aggression: {pa}\n",
        motto = current.motto,
        pa = p.aggression,
    ));

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::digest::{
        Culture, Digest, GarrisonEntry, MapCell, MapWindow, ReinforcementHere, ReportHead,
        ResourceLine, Resources, SlotLevel, VillageDigest,
    };

    fn persona_agg2() -> Persona {
        Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 2,
            raid_range: 10,
        }
    }

    fn minimal_digest() -> Digest {
        Digest {
            world: "world-0001".into(),
            player: "p1".into(),
            now_ms: 1_700_000_000_000,
            villages: vec![VillageDigest {
                id: "v-001".into(),
                x: 0,
                y: 0,
                resources: Resources {
                    wood: ResourceLine {
                        amount: 500,
                        rate: 30,
                        capacity: 1000,
                    },
                    clay: ResourceLine {
                        amount: 400,
                        rate: 25,
                        capacity: 1000,
                    },
                    iron: ResourceLine {
                        amount: 300,
                        rate: 20,
                        capacity: 1000,
                    },
                    crop: ResourceLine {
                        amount: 200,
                        rate: 10,
                        capacity: 1000,
                    },
                },
                fields: vec![SlotLevel {
                    slot: 0,
                    kind: "wood".into(),
                    level: 2,
                }],
                buildings: vec![SlotLevel {
                    slot: 0,
                    kind: "main_building".into(),
                    level: 3,
                }],
                garrison: vec![GarrisonEntry {
                    unit: "legionnaire".into(),
                    count: 20,
                }],
                ..Default::default()
            }],
            culture: Culture {
                cp: 50,
                rate_per_hour: 5,
                villages_used: 1,
                villages_allowed: 2,
                next_threshold: 200,
            },
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // parse_reply — per failure class
    // -----------------------------------------------------------------------

    #[test]
    fn parse_valid_minimal() {
        let raw = r#"{"focus":"balanced","motto":"hold steady"}"#;
        let reply = parse_reply(raw).expect("valid minimal should parse");
        assert_eq!(reply.strategy.focus, Focus::Balanced);
        assert_eq!(reply.strategy.motto, "hold steady");
        assert!(reply.strategy.aggression.is_none());
        assert!(reply.strategy.raid_quadrant.is_none());
        assert!(reply.message.is_none());
    }

    #[test]
    fn parse_valid_with_message() {
        let raw = r#"{"focus":"military","aggression":3,"motto":"crush them","message":{"to":"enemy","body":"prepare for war"}}"#;
        let reply = parse_reply(raw).expect("valid with message should parse");
        assert_eq!(reply.strategy.focus, Focus::Military);
        assert_eq!(reply.strategy.aggression, Some(3));
        let msg = reply.message.expect("message should be present");
        assert_eq!(msg.to, "enemy");
        assert_eq!(msg.body, "prepare for war");
    }

    #[test]
    fn parse_valid_all_quadrant_values() {
        for q in ["ne", "nw", "se", "sw"] {
            let raw = format!(
                r#"{{"focus":"balanced","motto":"x","raid_quadrant":"{q}","settle_quadrant":"{q}"}}"#
            );
            let reply = parse_reply(&raw).expect("valid quadrant should parse");
            assert!(reply.strategy.raid_quadrant.is_some());
            assert!(reply.strategy.settle_quadrant.is_some());
        }
    }

    #[test]
    fn parse_valid_economy_expansion() {
        for focus in ["economy", "expansion"] {
            let raw = format!(r#"{{"focus":"{focus}","motto":""}}"#);
            let reply = parse_reply(&raw).expect("valid focus");
            let _ = reply;
        }
    }

    #[test]
    fn parse_prose_wrapped_is_err() {
        // A reply wrapped in prose is an error — the prompt demands bare JSON.
        let raw = r#"Here is the JSON: {"focus":"balanced","motto":"x"}"#;
        let err = parse_reply(raw).expect_err("prose-wrapped should be Err");
        assert!(err.contains("not a bare JSON object"), "error was: {err}");
    }

    #[test]
    fn parse_fenced_is_err() {
        // A reply wrapped in markdown code fences is an error.
        let raw = "```json\n{\"focus\":\"balanced\",\"motto\":\"x\"}\n```";
        let err = parse_reply(raw).expect_err("fenced should be Err");
        assert!(err.contains("not a bare JSON object"), "error was: {err}");
    }

    #[test]
    fn parse_unknown_field_is_err() {
        let raw = r#"{"focus":"balanced","motto":"x","unknown_field":42}"#;
        let err = parse_reply(raw).expect_err("unknown field should be Err");
        // serde deny_unknown_fields produces "unknown field" in the error
        assert!(
            err.to_lowercase().contains("unknown field") || err.contains("JSON"),
            "error was: {err}"
        );
    }

    #[test]
    fn parse_focus_rush_is_err() {
        let raw = r#"{"focus":"rush","motto":"fast"}"#;
        let err = parse_reply(raw).expect_err("unknown focus should be Err");
        assert!(err.contains("unknown focus"), "error was: {err}");
    }

    #[test]
    fn parse_aggression_7_is_err() {
        let raw = r#"{"focus":"balanced","aggression":7,"motto":"x"}"#;
        let err = parse_reply(raw).expect_err("aggression 7 should be Err");
        assert!(
            err.contains("aggression") && err.contains("out of range"),
            "error was: {err}"
        );
    }

    #[test]
    fn parse_motto_200_chars_is_err() {
        let motto = "x".repeat(200);
        let raw = format!(r#"{{"focus":"balanced","motto":"{motto}"}}"#);
        let err = parse_reply(&raw).expect_err("motto > 80 chars should be Err");
        assert!(
            err.contains("motto") && err.contains("cap"),
            "error was: {err}"
        );
    }

    #[test]
    fn parse_motto_exactly_80_chars_ok() {
        let motto = "x".repeat(80);
        let raw = format!(r#"{{"focus":"balanced","motto":"{motto}"}}"#);
        parse_reply(&raw).expect("motto at exactly 80 chars should parse");
    }

    #[test]
    fn parse_whitespace_tolerance() {
        // Leading/trailing whitespace only must be tolerated.
        let raw = "  \n{\"focus\":\"economy\",\"motto\":\"frugal\"}\n  ";
        let reply = parse_reply(raw).expect("whitespace-padded should parse");
        assert_eq!(reply.strategy.focus, Focus::Economy);
    }

    #[test]
    fn parse_message_empty_to_is_err() {
        let raw = r#"{"focus":"balanced","motto":"x","message":{"to":"","body":"hello"}}"#;
        let err = parse_reply(raw).expect_err("empty to should be Err");
        assert!(err.contains("to"), "error was: {err}");
    }

    #[test]
    fn parse_message_body_too_long_is_err() {
        let body = "x".repeat(501);
        let raw = format!(
            r#"{{"focus":"balanced","motto":"x","message":{{"to":"user","body":"{body}"}}}}"#
        );
        let err = parse_reply(&raw).expect_err("body > 500 chars should be Err");
        assert!(
            err.contains("body") || err.contains("500"),
            "error was: {err}"
        );
    }

    #[test]
    fn parse_message_body_exactly_500_chars_ok() {
        let body = "x".repeat(500);
        let raw = format!(
            r#"{{"focus":"balanced","motto":"x","message":{{"to":"user","body":"{body}"}}}}"#
        );
        parse_reply(&raw).expect("body at exactly 500 chars should parse");
    }

    // -----------------------------------------------------------------------
    // in_quadrant — quadrant convention
    // -----------------------------------------------------------------------

    #[test]
    fn in_quadrant_convention() {
        // positive x = east, negative y = north
        assert!(in_quadrant(1, -1, &Quadrant::NE), "NE: dx>0, dy<0");
        assert!(
            in_quadrant(5, -3, &Quadrant::NE),
            "NE: large positive dx, negative dy"
        );
        assert!(in_quadrant(-1, -1, &Quadrant::NW), "NW: dx<0, dy<0");
        assert!(in_quadrant(1, 1, &Quadrant::SE), "SE: dx>0, dy>0");
        assert!(in_quadrant(-1, 1, &Quadrant::SW), "SW: dx<0, dy>0");
    }

    #[test]
    fn in_quadrant_axis_zero_never_matches() {
        // dx==0 or dy==0 → non-match for all quadrants
        for q in [&Quadrant::NE, &Quadrant::NW, &Quadrant::SE, &Quadrant::SW] {
            assert!(!in_quadrant(0, 5, q), "dx=0 should not match {:?}", q);
            assert!(!in_quadrant(5, 0, q), "dy=0 should not match {:?}", q);
            assert!(!in_quadrant(0, 0, q), "origin should not match {:?}", q);
        }
    }

    #[test]
    fn in_quadrant_no_cross_match() {
        // A cell clearly in NE should not match NW, SE, SW.
        assert!(!in_quadrant(1, -1, &Quadrant::NW));
        assert!(!in_quadrant(1, -1, &Quadrant::SE));
        assert!(!in_quadrant(1, -1, &Quadrant::SW));
    }

    // -----------------------------------------------------------------------
    // build_prompt — determinism, caps, fog boundary
    // -----------------------------------------------------------------------

    #[test]
    fn prompt_deterministic_same_fixture_twice() {
        let d = minimal_digest();
        let p = persona_agg2();
        let s = Strategy::default();
        let p1 = build_prompt(&d, None, &p, &s, "AlphaBot", BotTribe::Romans);
        let p2 = build_prompt(&d, None, &p, &s, "AlphaBot", BotTribe::Romans);
        assert_eq!(p1, p2, "same inputs must produce identical prompt");
    }

    #[test]
    fn prompt_caps_reports_at_5() {
        let mut d = minimal_digest();
        // 20 report heads — only 5 should appear in the prompt.
        d.reports = (0..20_i64)
            .map(|i| ReportHead {
                id: i.to_string(),
                occurred_at_ms: 1_700_000_000_000 + i * 1_000,
                attacker_won: i % 2 == 0,
                kind: "raid".into(),
            })
            .collect();

        let p = persona_agg2();
        let s = Strategy::default();
        let prompt = build_prompt(&d, None, &p, &s, "BetaBot", BotTribe::Teutons);

        // Each report line contains "occurred=" exactly once.
        let report_count = prompt.matches("occurred=").count();
        assert_eq!(
            report_count, 5,
            "only 5 reports should appear; got {report_count}"
        );
    }

    #[test]
    fn prompt_caps_map_cells_at_10() {
        let d = minimal_digest();
        // Map with 20 cells — only 10 should appear.
        let cells: Vec<MapCell> = (0_i32..20)
            .map(|i| MapCell {
                cell_class: "".into(),
                label: format!("cell-{i}"),
                href: None,
                settle: false,
                x: i,
                y: 0,
            })
            .collect();
        let map = MapWindow {
            center_x: 0,
            center_y: 0,
            r: 20,
            rows: vec![cells],
        };

        let p = persona_agg2();
        let s = Strategy::default();
        let prompt = build_prompt(&d, Some(&map), &p, &s, "BetaBot", BotTribe::Teutons);

        // Each map-cell line contains "dist=" exactly once.
        let cell_count = prompt.matches("dist=").count();
        assert_eq!(
            cell_count, 10,
            "only 10 map cells should appear; got {cell_count}"
        );
    }

    #[test]
    fn prompt_no_reinforcements_here_leakage() {
        // A digest with reinforcements_here (foreign troops at our village).
        // The prompt must NOT expose these troops (fog boundary).
        let mut d = minimal_digest();
        d.villages[0].reinforcements_here = vec![ReinforcementHere {
            home_village: "enemy-uuid-secret".into(),
            x: 5,
            y: 5,
            owner: "foe".into(),
            troops: {
                let mut m = HashMap::new();
                m.insert("legionnaire".into(), 99u32);
                m
            },
        }];

        let p = persona_agg2();
        let s = Strategy::default();
        let prompt = build_prompt(&d, None, &p, &s, "GammaBot", BotTribe::Gauls);

        // The foreign village UUID must not appear in the prompt.
        assert!(
            !prompt.contains("enemy-uuid-secret"),
            "prompt must not contain foreign village UUID"
        );
        // The reinforcements_here field name must not appear.
        assert!(
            !prompt.contains("reinforcements_here"),
            "prompt must not leak the reinforcements_here field name"
        );
    }

    #[test]
    fn prompt_contains_current_strategy_and_persona_aggression() {
        let d = minimal_digest();
        let p = persona_agg2();
        let s = Strategy {
            focus: Focus::Military,
            aggression: Some(3),
            motto: "crush all".into(),
            ..Strategy::default()
        };
        let prompt = build_prompt(&d, None, &p, &s, "DeltaBot", BotTribe::Romans);
        assert!(prompt.contains("focus=military"), "focus should appear");
        assert!(
            prompt.contains("aggression=3"),
            "aggression override should appear"
        );
        assert!(prompt.contains("crush all"), "motto should appear");
        assert!(
            prompt.contains("Persona base aggression: 2"),
            "persona base aggression should appear"
        );
    }

    #[test]
    fn prompt_different_tribes_different_strings() {
        let d = minimal_digest();
        let p = persona_agg2();
        let s = Strategy::default();
        let p_roman = build_prompt(&d, None, &p, &s, "Bot", BotTribe::Romans);
        let p_teuton = build_prompt(&d, None, &p, &s, "Bot", BotTribe::Teutons);
        assert_ne!(p_roman, p_teuton, "tribe name should differ");
        assert!(p_roman.contains("Roman"));
        assert!(p_teuton.contains("Teuton"));
    }
}
