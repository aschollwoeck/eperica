//! The in-game player manual (127 T1): a compile-time chapter registry over the prose files in
//! `docs/manual/`, rendered server-side (pulldown-cmark) into the game chrome.
//!
//! `docs/manual/*.md` is the single source read both by repo readers (GitHub) and this site — no
//! content duplication (plan "Hybrid content pipeline"). Chapters are embedded with `include_str!`
//! so a renamed/missing file is a **build** error, never a runtime 404 surprise (plan "Key
//! decisions": "a missing file is a build error, a dead slug is impossible").
//!
//! Reference pages (Units/Buildings/Mechanics numbers, generated from `WorldRules`) are a separate
//! T2 concern; this module only carries their sidebar [`RefLink`]s so the Reference section can
//! list them now (they may 404 until T2 lands — acceptable within this branch, per 127 tasks.md).

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

/// One manual chapter: its route slug, nav title, and raw markdown body (embedded at compile
/// time — a missing `docs/manual/<slug>.md` fails the build).
pub struct Chapter {
    pub slug: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

/// A top-level section grouping chapters in sidebar/registry order (spec "Guided structure").
pub struct Section {
    pub title: &'static str,
    pub chapters: &'static [Chapter],
}

/// A generated reference page announced in the Reference section (T2 delivers the actual
/// `/manual/reference/{slug}` routes).
pub struct RefLink {
    pub slug: &'static str,
    pub title: &'static str,
}

/// Defines one [`Chapter`], embedding `docs/manual/<slug>.md` relative to this source file.
macro_rules! chapter {
    ($slug:literal, $title:literal) => {
        Chapter {
            slug: $slug,
            title: $title,
            body: include_str!(concat!("../../../docs/manual/", $slug, ".md")),
        }
    };
}

/// The canonical section/chapter registry (127 spec "Guided structure" — six sections replacing
/// the flat index). Order here is the sidebar order, the prev/next chain order, and the order the
/// README mirror test checks against.
pub static SECTIONS: &[Section] = &[
    Section {
        title: "Getting started",
        chapters: &[
            chapter!("getting-started", "Getting started"),
            chapter!("worlds", "Worlds & game modes"),
            chapter!("quests-and-onboarding", "Quests & onboarding"),
            chapter!("the-map", "The world map"),
        ],
    },
    Section {
        title: "Economy",
        chapters: &[
            chapter!("resources", "Resources"),
            chapter!("buildings", "Building & upgrading"),
            chapter!("trade", "Trading"),
        ],
    },
    Section {
        title: "Military",
        chapters: &[
            chapter!("tribes-and-units", "Tribes, the Academy & the Smithy"),
            chapter!("training-and-upkeep", "Training troops & feeding your army"),
            chapter!("troop-movement", "Moving troops"),
            chapter!("combat", "Attacking & defending"),
            chapter!("scouting", "Scouting"),
            chapter!("siege-and-loot", "Siege & loot"),
            chapter!("oases", "Oases"),
        ],
    },
    Section {
        title: "Expansion",
        chapters: &[
            chapter!("settling", "Settling"),
            chapter!("conquest", "Conquest"),
            chapter!("protection-and-lifecycle", "Protection & a living world"),
        ],
    },
    Section {
        title: "Society",
        chapters: &[
            chapter!("alliances", "Alliances & diplomacy"),
            chapter!("alliance-forum", "The alliance forum"),
            chapter!("communication", "Communication — messages & chat"),
            chapter!("notifications", "Notifications & alerts"),
            chapter!("profiles-and-presence", "Your profile & who's online"),
            chapter!("search", "Finding players, alliances & places"),
            chapter!("settings", "Settings & preferences"),
            chapter!("account-sitting", "Account sitting"),
            chapter!("ranking-and-statistics", "Ranking & statistics"),
            chapter!("medals-and-achievements", "Medals & achievements"),
            chapter!("fair-play-and-moderation", "Fair play & moderation"),
            chapter!("ai-players", "AI players (NPCs)"),
            chapter!("spectating", "Spectating"),
        ],
    },
    Section {
        title: "Reference",
        chapters: &[
            chapter!("artifacts", "Artifacts & the Natars"),
            chapter!("wonder-and-victory", "The Wonder of the World & victory"),
        ],
    },
];

/// The three generated reference pages (T2), listed in the Reference section alongside the prose
/// end-game chapters.
pub static REFERENCE_LINKS: &[RefLink] = &[
    RefLink {
        slug: "units",
        title: "Full unit stats",
    },
    RefLink {
        slug: "buildings",
        title: "All buildings & prerequisites",
    },
    RefLink {
        slug: "mechanics",
        title: "The numbers",
    },
];

/// A lightweight prev/next sidebar link (no body — just enough to render a nav card).
pub struct NavLink {
    pub slug: &'static str,
    pub title: &'static str,
}

/// A fully rendered chapter, ready for the template: HTML body plus the nav context (breadcrumb
/// section title, prev/next).
pub struct RenderedChapter {
    pub title: &'static str,
    pub section: &'static str,
    pub html: String,
    pub prev: Option<NavLink>,
    pub next: Option<NavLink>,
}

/// All chapters flattened in registry order, each paired with its owning section — the single
/// source for lookup, the prev/next chain, and the slug list.
fn flat_chapters() -> Vec<(&'static Section, &'static Chapter)> {
    SECTIONS
        .iter()
        .flat_map(|s| s.chapters.iter().map(move |c| (s, c)))
        .collect()
}

/// Every registered chapter slug, in registry order — exposed for integration tests (AC1: every
/// registered chapter must 200) and the README-mirror check.
pub fn all_slugs() -> Vec<&'static str> {
    flat_chapters().into_iter().map(|(_, c)| c.slug).collect()
}

/// Renders the chapter at `slug`, or `None` if no such chapter is registered (the caller turns
/// that into the site's normal 404 — AC1).
pub fn render(slug: &str) -> Option<RenderedChapter> {
    let flat = flat_chapters();
    let idx = flat.iter().position(|(_, c)| c.slug == slug)?;
    let (section, chapter) = flat[idx];
    let html = render_markdown(chapter.body);
    let prev = idx.checked_sub(1).map(|i| NavLink {
        slug: flat[i].1.slug,
        title: flat[i].1.title,
    });
    let next = flat.get(idx + 1).map(|(_, c)| NavLink {
        slug: c.slug,
        title: c.title,
    });
    Some(RenderedChapter {
        title: chapter.title,
        section: section.title,
        html,
        prev,
        next,
    })
}

/// The three callout kinds (spec "A manual that looks like the game" — `> **Tip:** …` convention)
/// and the CSS class each gets, in match-priority order.
const CALLOUT_KINDS: [(&str, &str); 3] = [
    ("Tip:", "co-tip"),
    ("Warning:", "co-warn"),
    ("Faithful:", "co-faith"),
];

/// Markdown → HTML for one chapter body (AC2/AC7 — cheap, no I/O, run per request from the
/// in-memory embedded source).
fn render_markdown(src: &str) -> String {
    let opts = Options::ENABLE_TABLES;
    let events: Vec<Event> = Parser::new_ext(src, opts).collect();
    let events = transform(events);
    let mut out = String::with_capacity(src.len() * 2);
    html::push_html(&mut out, events.into_iter());
    out
}

/// Rewrites the parsed event stream before rendering:
/// - heading `id`s on H2/H3 (slugified from their text) so "See also" cross-links can target them;
/// - intra-manual link targets (`foo.md`, `foo.md#anchor`) rewritten to `/manual/foo[#anchor]`;
/// - callout blockquotes (`> **Tip:** …`) get `class="co co-tip|co-warn|co-faith"`;
/// - **raw/inline HTML is never passed through verbatim** (AC2): `Html`/`InlineHtml` events are
///   remapped to `Text`, which the renderer HTML-escapes like any other text. `pulldown-cmark` has
///   no single "disable raw HTML" `Options` flag — its standard `html::push_html` writes `Html`/
///   `InlineHtml` events unescaped by design, so disabling passthrough means intercepting those
///   two event kinds ourselves, which is what this does. Any `Event::Html`/`InlineHtml` **we**
///   synthesize afterwards (the callout `<blockquote class="…">` markers) is emitted directly as
///   `Event::Html` and is therefore never subject to this remapping — it is our own fixed, trusted
///   string, not source content.
fn transform<'a>(events: Vec<Event<'a>>) -> Vec<Event<'a>> {
    let mut out = Vec::with_capacity(events.len());
    let mut i = 0;
    // Blockquote nesting depth (mirrors `classify_callout`'s own tracking) — the label-stripping
    // below only ever fires at depth 1, the same scope a callout's classifying strong run lives in.
    let mut quote_depth = 0i32;
    // Armed with the callout's label text (e.g. "Tip:") right after entering a classified callout
    // blockquote; disarmed the moment the matching `Strong` run is stripped. `strip_leading_space`
    // then eats the one space between the removed label and the rest of the sentence.
    let mut strip_label: Option<&'static str> = None;
    let mut strip_leading_space = false;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) if matches!(level, HeadingLevel::H2 | HeadingLevel::H3) => {
                let id = slugify(&heading_text(&events, i + 1));
                out.push(Event::Start(Tag::Heading {
                    level: *level,
                    id: Some(CowStr::from(id)),
                    classes: classes.clone(),
                    attrs: attrs.clone(),
                }));
                i += 1;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                let class = classify_callout(&events, i);
                match class {
                    Some(c) => {
                        out.push(Event::Html(CowStr::from(format!(
                            "<blockquote class=\"co {c}\">\n"
                        ))));
                        // The CSS `::before` on `.co-*` already prints the label ("Tip"/"Warning"/
                        // "Faithful") — arm the strip so the redundant `**Tip:**` bold prefix in the
                        // prose itself doesn't also render (NIT, 127 review: "double label").
                        strip_label = CALLOUT_KINDS
                            .iter()
                            .find(|(_, kind)| *kind == c)
                            .map(|(prefix, _)| *prefix);
                    }
                    None => out.push(Event::Html(CowStr::from("<blockquote>\n"))),
                }
                quote_depth += 1;
                i += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                out.push(Event::Html(CowStr::from("</blockquote>\n")));
                quote_depth -= 1;
                if quote_depth == 0 {
                    strip_label = None;
                }
                i += 1;
            }
            // The first top-level Strong run inside an armed callout is exactly the label
            // `classify_callout` matched (same depth, same "first strong" rule) — drop the whole
            // run (`Start(Strong)` .. `End(Strong)`, nested strongs included) rather than emit it.
            Event::Start(Tag::Strong) if quote_depth == 1 && strip_label.is_some() => {
                let mut j = i + 1;
                let mut strong_depth = 1i32;
                while j < events.len() && strong_depth > 0 {
                    match &events[j] {
                        Event::Start(Tag::Strong) => strong_depth += 1,
                        Event::End(TagEnd::Strong) => strong_depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                i = j;
                strip_label = None;
                strip_leading_space = true;
            }
            Event::Text(t) if strip_leading_space => {
                out.push(Event::Text(CowStr::from(
                    t.strip_prefix(' ').unwrap_or(t).to_owned(),
                )));
                strip_leading_space = false;
                i += 1;
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                out.push(Event::Start(Tag::Link {
                    link_type: *link_type,
                    dest_url: CowStr::from(rewrite_link(dest_url)),
                    title: title.clone(),
                    id: id.clone(),
                }));
                i += 1;
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                out.push(Event::Text(t.clone()));
                i += 1;
            }
            other => {
                out.push(other.clone());
                i += 1;
            }
        }
    }
    out
}

/// Collects the plain text of a heading (Text/Code events) starting just after its `Start` event,
/// stopping at the matching `End(Heading)`.
fn heading_text(events: &[Event], mut i: usize) -> String {
    let mut s = String::new();
    while i < events.len() {
        match &events[i] {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Text(t) | Event::Code(t) => s.push_str(t),
            _ => {}
        }
        i += 1;
    }
    s
}

/// A URL-safe anchor slug: lowercase alphanumerics joined by single hyphens, trimmed of leading/
/// trailing hyphens.
fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_dash = true;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Classifies the blockquote whose `Start` event sits at `events[start]`: `Some(class)` when its
/// first top-level **strong** run starts with `Tip:`/`Warning:`/`Faithful:`, `None` otherwise
/// (including plain quotes, nested blockquotes, and quotes whose first strong text doesn't match).
fn classify_callout(events: &[Event], start: usize) -> Option<&'static str> {
    let mut depth = 0i32;
    let mut in_strong = false;
    let mut i = start;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::BlockQuote(_)) => depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
            }
            Event::Start(Tag::Strong) if depth == 1 => in_strong = true,
            Event::End(TagEnd::Strong) if depth == 1 => in_strong = false,
            Event::Text(t) if depth == 1 && in_strong => {
                return CALLOUT_KINDS
                    .iter()
                    .find(|(prefix, _)| t.starts_with(prefix))
                    .map(|(_, class)| *class);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Rewrites an intra-manual link target for the rendered `href`; anything that isn't a bare
/// `<slug>.md[#anchor]` (external URLs, absolute paths, bare anchors) passes through unchanged.
fn rewrite_link(dest: &str) -> String {
    match rewrite_target(dest) {
        Some(RewriteTarget::Home) => "/manual".to_owned(),
        Some(RewriteTarget::Chapter(t)) => format!("/manual/{t}"),
        None => dest.to_owned(),
    }
}

/// Where a `foo.md`/`foo.md#anchor`/`README.md` link target rewrites to.
#[derive(Debug, PartialEq, Eq)]
enum RewriteTarget {
    /// `README.md` (± anchor — the anchor is dropped, the index has no headings of its own) — the
    /// manual's own contents page isn't a registered chapter, so it rewrites straight to `/manual`
    /// rather than a dead `/manual/README`.
    Home,
    /// An ordinary registered chapter slug (+ optional `#anchor`).
    Chapter(String),
}

/// The rewrite of a `foo.md`/`foo.md#anchor` link target, or `None` if `target` isn't that shape
/// (external `://` URL, absolute `/…` path, bare `#anchor`, or a filename with characters outside
/// `[a-z0-9_-]` — every registered chapter slug is lowercase, so anything else can never resolve).
/// `README.md` is the one special case, matched before that lowercase check (`README` itself is
/// uppercase by convention, and isn't a registered slug at all — see [`RewriteTarget::Home`]).
fn rewrite_target(target: &str) -> Option<RewriteTarget> {
    if target.is_empty()
        || target.contains("://")
        || target.starts_with('/')
        || target.starts_with('#')
    {
        return None;
    }
    let (path, anchor) = match target.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (target, None),
    };
    let slug = path.strip_suffix(".md")?;
    if slug == "README" {
        return Some(RewriteTarget::Home);
    }
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return None;
    }
    Some(RewriteTarget::Chapter(match anchor {
        Some(a) if !a.is_empty() => format!("{slug}#{a}"),
        _ => slug.to_owned(),
    }))
}

/// Every `href="/manual…"` target rendered into `html`, in appearance order (127 review M1) — the
/// dead-link regression test scans these across the whole corpus so a broken intra-manual link is a
/// **build-time test failure**, never a live 404 a reader stumbles into. Test-only (no production
/// caller), so it's gated the same way the rest of the test-only helpers below are.
#[cfg(test)]
fn manual_hrefs(html: &str) -> Vec<&str> {
    const NEEDLE: &str = "href=\"/manual";
    let mut out = Vec::new();
    let mut scanned = 0usize;
    while let Some(rel) = html[scanned..].find(NEEDLE) {
        let value_start = scanned + rel + "href=\"".len();
        let end = html[value_start..]
            .find('"')
            .map_or(html.len(), |e| value_start + e);
        out.push(&html[value_start..end]);
        scanned = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn slugs_are_unique() {
        let slugs = all_slugs();
        let unique: HashSet<_> = slugs.iter().collect();
        assert_eq!(
            unique.len(),
            slugs.len(),
            "duplicate slug in the manual registry: {slugs:?}"
        );
    }

    #[test]
    fn every_registered_chapter_is_non_empty_and_starts_with_an_h1() {
        for slug in all_slugs() {
            let rendered = render(slug).unwrap_or_else(|| panic!("{slug} must render"));
            assert!(!rendered.html.trim().is_empty(), "{slug} rendered empty");
            // The raw source (not the HTML) must start with a markdown H1 — the chapter's title
            // heading, one per file.
            let (_, chapter) = flat_chapters()
                .into_iter()
                .find(|(_, c)| c.slug == slug)
                .unwrap();
            assert!(
                chapter.body.trim_start().starts_with("# "),
                "{slug} must start with a markdown H1"
            );
            // No unrendered markdown-link artifact should survive into the HTML.
            assert!(
                !rendered.html.contains("]("),
                "{slug} left an unrendered markdown link: {}",
                rendered.html
            );
        }
    }

    #[test]
    fn render_unknown_slug_is_none() {
        assert!(render("does-not-exist").is_none());
    }

    #[test]
    fn link_rewriting_handles_anchor_and_bare_targets() {
        assert_eq!(
            rewrite_target("resources.md"),
            Some(RewriteTarget::Chapter("resources".to_owned()))
        );
        assert_eq!(
            rewrite_target("resources.md#storage"),
            Some(RewriteTarget::Chapter("resources#storage".to_owned()))
        );
    }

    #[test]
    fn link_rewriting_leaves_external_and_absolute_targets_untouched() {
        assert_eq!(rewrite_target("https://example.com/x.md"), None);
        assert_eq!(rewrite_target("http://example.com"), None);
        assert_eq!(rewrite_target("/manual/reference/units"), None);
        assert_eq!(rewrite_target("#anchor-only"), None);
        assert_eq!(rewrite_target("mailto:a@b.com"), None);
    }

    /// M1 (127 review): `README.md` — the manual's own contents page, linked from every chapter's
    /// footer — isn't a registered chapter (it has no slug), so it must not become a dead
    /// `/manual/README`. The anchor (if any) is dropped; the index has no headings to target.
    #[test]
    fn link_rewriting_special_cases_readme_to_the_manual_index() {
        assert_eq!(rewrite_target("README.md"), Some(RewriteTarget::Home));
        assert_eq!(
            rewrite_target("README.md#contents"),
            Some(RewriteTarget::Home)
        );
        let html = render_markdown("See the [index](README.md).");
        assert!(html.contains(r#"href="/manual""#), "{html}");
        assert!(!html.contains("/manual/README"), "{html}");
    }

    /// M1 (127 review): the doc comment on `rewrite_target` claims `[a-z0-9_-]` — the code
    /// previously accepted uppercase too (`is_ascii_alphanumeric`), which was never exercised
    /// because every real chapter slug is lowercase. Pin the doc's stricter behavior: an uppercase
    /// filename (other than the special-cased `README`) does not resolve.
    #[test]
    fn link_rewriting_rejects_uppercase_outside_the_readme_special_case() {
        assert_eq!(rewrite_target("Resources.md"), None);
        assert_eq!(rewrite_target("RESOURCES.md"), None);
    }

    #[test]
    fn rendered_intra_manual_link_points_at_manual_route() {
        let html = render_markdown("See [Resources](resources.md#storage) for more.");
        assert!(
            html.contains(r#"href="/manual/resources#storage""#),
            "{html}"
        );
        assert!(!html.contains("]("), "{html}");
    }

    #[test]
    fn rendered_external_link_is_untouched() {
        let html = render_markdown("See [the site](https://example.com/docs).");
        assert!(
            html.contains(r#"href="https://example.com/docs""#),
            "{html}"
        );
    }

    /// M1 (127 review), the class-killing test: every `href="/manual…"` rendered anywhere in the
    /// whole chapter corpus must resolve — either the bare index, a registered chapter slug, or one
    /// of the three generated reference pages. A future chapter that links a renamed/removed slug
    /// fails *this* test, not a reader's click.
    #[test]
    fn every_rendered_manual_href_resolves() {
        let slugs: HashSet<&str> = all_slugs().into_iter().collect();
        for slug in all_slugs() {
            let rendered = render(slug).unwrap();
            for href in manual_hrefs(&rendered.html) {
                if href == "/manual" {
                    continue;
                }
                let Some(rest) = href.strip_prefix("/manual/") else {
                    panic!("{slug}: malformed manual href {href}");
                };
                let target = rest.split('#').next().unwrap_or(rest);
                let resolves = slugs.contains(target)
                    || matches!(
                        target,
                        "reference/units" | "reference/buildings" | "reference/mechanics"
                    );
                assert!(resolves, "{slug} links to unresolvable {href}");
            }
            // A .md-shaped link that FAILED rewriting escapes the /manual prefix scan as a
            // relative href — catch that class too: no rendered href may end in .md.
            assert!(
                !rendered.html.contains(".md\""),
                "{slug} contains an unrewritten .md href"
            );
        }
    }

    /// Every callout the classifier recognises must carry EXACTLY the bare label as its first
    /// strong run — an author writing `> **Tip: don't do X**` would otherwise have the whole
    /// bold (including prose) silently stripped by the label-dedup pass.
    #[test]
    fn every_corpus_callout_label_is_exactly_the_bare_label() {
        for (_, ch) in flat_chapters() {
            let slug = ch.slug;
            for line in ch.body.lines() {
                let t = line.trim_start();
                let Some(rest) = t.strip_prefix("> **") else {
                    continue;
                };
                for label in ["Tip", "Warning", "Faithful"] {
                    if rest.starts_with(label) {
                        assert!(
                            rest.starts_with(&format!("{label}:**")),
                            "{slug}: callout label must be exactly **{label}:** — got: {t}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn callout_classing_recognizes_all_three_kinds() {
        let tip = render_markdown("> **Tip:** do the thing.");
        assert!(tip.contains(r#"class="co co-tip""#), "{tip}");

        let warn = render_markdown("> **Warning:** don't do the thing.");
        assert!(warn.contains(r#"class="co co-warn""#), "{warn}");

        let faith = render_markdown("> **Faithful:** just like the original.");
        assert!(faith.contains(r#"class="co co-faith""#), "{faith}");
    }

    /// NIT (127 review): the CSS `::before` on `.co-tip`/`.co-warn`/`.co-faith` already prints the
    /// label, so the rendered body must not also show the bold `**Tip:**`/`**Warning:**`/
    /// `**Faithful:**` prefix — that was a double label. The rest of the sentence (and its own
    /// bold text, if any) survives untouched.
    #[test]
    fn callout_label_prefix_is_stripped_once_not_duplicated() {
        let tip = render_markdown("> **Tip:** do the **important** thing.");
        assert!(!tip.contains("Tip:"), "{tip}");
        assert!(
            tip.contains("do the <strong>important</strong> thing."),
            "{tip}"
        );
        assert!(tip.contains(r#"class="co co-tip""#), "{tip}");

        let warn = render_markdown("> **Warning:** don't do the thing.");
        assert!(!warn.contains("Warning:"), "{warn}");
        assert!(warn.contains("don't do the thing."), "{warn}");

        let faith = render_markdown("> **Faithful:** just like the original.");
        assert!(!faith.contains("Faithful:"), "{faith}");
        assert!(faith.contains("just like the original."), "{faith}");

        // A non-matching blockquote keeps its bold text exactly as written — only the classifying
        // label prefix is ever stripped.
        let other_strong = render_markdown("> **Not a callout.** Some more text.");
        assert!(
            other_strong.contains("<strong>Not a callout.</strong>"),
            "{other_strong}"
        );
    }

    #[test]
    fn callout_classing_ignores_non_matching_blockquotes() {
        let plain = render_markdown("> Just a quote, no convention.");
        assert!(!plain.contains("class=\"co"), "{plain}");
        assert!(plain.contains("<blockquote>"), "{plain}");

        let other_strong = render_markdown("> **Not a callout.** Some more text.");
        assert!(!other_strong.contains("class=\"co"), "{other_strong}");
    }

    #[test]
    fn inline_and_raw_html_is_escaped_not_injected() {
        let html = render_markdown("Before <script>alert(1)</script> after.\n\n<div>block</div>\n");
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("<div>block</div>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn heading_anchors_are_slugified() {
        let html = render_markdown("## The Rally Point & You\n\nbody");
        assert!(html.contains(r#"id="the-rally-point-you""#), "{html}");
    }

    #[test]
    fn prev_next_chain_is_correct_including_across_section_boundaries() {
        let flat = flat_chapters();
        for (idx, (_, chapter)) in flat.iter().enumerate() {
            let rendered = render(chapter.slug).unwrap();
            match idx.checked_sub(1) {
                Some(pi) => assert_eq!(rendered.prev.unwrap().slug, flat[pi].1.slug),
                None => assert!(rendered.prev.is_none()),
            }
            match flat.get(idx + 1) {
                Some((_, next_chapter)) => {
                    assert_eq!(rendered.next.unwrap().slug, next_chapter.slug)
                }
                None => assert!(rendered.next.is_none()),
            }
        }
        // Explicitly cross a section boundary: the last chapter of "Getting started" (the-map) is
        // followed by the first chapter of "Economy" (resources).
        let last_getting_started = SECTIONS[0].chapters.last().unwrap();
        let rendered = render(last_getting_started.slug).unwrap();
        assert_eq!(rendered.next.unwrap().slug, SECTIONS[1].chapters[0].slug);
    }

    #[test]
    fn reference_links_are_present() {
        assert_eq!(REFERENCE_LINKS.len(), 3);
        let slugs: Vec<_> = REFERENCE_LINKS.iter().map(|r| r.slug).collect();
        assert_eq!(slugs, ["units", "buildings", "mechanics"]);
    }

    /// M2 (127 review, AC1): `docs/manual/README.md` is the repo-reader's contents page (GitHub never
    /// runs this code, so it needs its own hand-maintained `## Contents`) — it must mirror the
    /// registry it's a manual copy of, or the two drift apart unnoticed. Parses the six `### N. Title`
    /// sections and their `](slug.md)`-style bullet links (in order) and asserts each section's title
    /// and slug list matches [`SECTIONS`] exactly. The three generated reference pages are linked from
    /// README as absolute `/manual/reference/...` paths (not `.md`), so they're naturally excluded from
    /// this `.md`-link scan — consistent with the registry's own `REFERENCE_LINKS` being separate from
    /// `SECTIONS`.
    #[test]
    fn readme_contents_mirrors_the_chapter_registry() {
        const README: &str = include_str!("../../../docs/manual/README.md");

        let mut readme_sections: Vec<(String, Vec<String>)> = Vec::new();
        let mut current: Option<(String, Vec<String>)> = None;
        for line in README.lines() {
            if let Some(heading) = line.strip_prefix("### ") {
                if let Some(done) = current.take() {
                    readme_sections.push(done);
                }
                // "1. Getting started" -> "Getting started".
                let title = heading
                    .split_once(". ")
                    .map_or(heading, |(_, t)| t)
                    .to_owned();
                current = Some((title, Vec::new()));
            } else if let Some((_, slugs)) = current.as_mut() {
                let mut rest = line;
                while let Some(open) = rest.find("](") {
                    let after = &rest[open + 2..];
                    let close = after.find(')').unwrap_or(after.len());
                    let target = &after[..close];
                    if let Some(slug) = target.strip_suffix(".md") {
                        slugs.push(slug.to_owned());
                    } else if let Some((slug, _anchor)) = target.split_once(".md#") {
                        slugs.push(slug.to_owned());
                    }
                    rest = &after[close..];
                }
            }
        }
        if let Some(done) = current.take() {
            readme_sections.push(done);
        }

        assert_eq!(
            readme_sections.len(),
            SECTIONS.len(),
            "README's section count must mirror the registry: {readme_sections:?}"
        );
        for ((readme_title, readme_slugs), section) in readme_sections.iter().zip(SECTIONS.iter()) {
            assert_eq!(
                readme_title, section.title,
                "README section heading order/text must mirror the registry"
            );
            let registry_slugs: Vec<&str> = section.chapters.iter().map(|c| c.slug).collect();
            assert_eq!(
                readme_slugs.iter().map(String::as_str).collect::<Vec<_>>(),
                registry_slugs,
                "README's {} section must link the same chapters, same order, as the registry",
                section.title
            );
        }
    }
}
