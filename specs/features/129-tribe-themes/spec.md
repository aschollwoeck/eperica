# Feature 129 — tribe themes: a full skin + signature animations per tribe

**Status:** Draft
**Depends on:** 042/045 (per-world tribes), 115/116 (current chrome), the design direction note
(stylish, characterful; styles-only).
**Origin:** operator request — "different styles depending on the tribe", aligned: full themed
skin + animations; game pages themed by the acting player's tribe in the selected world; Romans
in a sepia direction that matches the shipped art.

## Goal

Playing a tribe should *feel* like that tribe everywhere you act as a player: a coherent skin
(palette, borders, panels, headers, tables, nav accents) plus characterful motion — signature
primary-action buttons and ornament details per tribe — while the neutral identity of public
surfaces (lobby, manual, boards, register) stays untouched.

## The three directions

| Tribe | Mood | Palette | Shape & texture | Signature button (primary actions) |
|---|---|---|---|---|
| **Romans** | Imperial, aged parchment — **sepia** to match the painted plates | Warm sepia/umber grounds, gold accents, muted crimson highlights | Clean straight edges, marble-light gradients, laurel details | **Cape swing** — a crimson drape pseudo-element that sways on hover |
| **Teutons** | Iron and ember | Cold steel greys, ember red-orange accents | Heavy blunt shapes, dark rough-timber gradients, rune details | **Forge ember** — a smoldering glow that pulses from the edges on hover |
| **Gauls** | Wild, druidic | Forest greens, bronze accents, moss/mist grounds | Rounded organic corners, knotwork details | **Leaf unfurl** — a bronze-green sweep that unfurls across on hover |

## Concepts

- **Token overrides, zero layout.** The game chrome already runs on CSS custom properties; each
  theme is a `[data-theme="<tribe>"]` block overriding tokens + a bounded set of component rules.
  No template layout changes; the only DOM additions are the theme attribute and (at most)
  classes on existing elements. A wrong theme can mis-color, never break, a page.
- **Per-world tribe, house mechanism.** User-specific chrome in `base.html` is already driven by
  the `/me` probe (admin/spectate links); the theme rides the same probe: `/me` exposes the
  selected world's tribe, a small script sets `data-theme` on the root element **only on
  world-scoped paths** (`/w/…`), and caches the last theme in `localStorage` to repaint
  flash-free on navigation. Public/neutral pages never get the attribute. Cosmetic only — no
  P4 surface.
- **Motion with restraint.** Signature animations are hover/entrance flourishes (the button
  drapes, ember pulses, unfurls; ornamented panel headers with a subtle entrance), all
  CSS-only, all gated behind `@media (prefers-reduced-motion: no-preference)`.
- **Ornaments are CSS/SVG-data assets** (laurel / rune / knot as inline `data:` URIs or drawn
  gradients) — no new binary art required; the existing tribal plates stay the pictorial layer.

## Acceptance criteria

- **AC1 — Theme wiring.** `/me` carries the selected world's tribe; on world-scoped pages the
  root element gets `data-theme="romans|teutons|gauls"` matching the acting player's tribe in
  that world (switching worlds switches the theme); lobby/manual/public boards/register never
  carry the attribute. Integration-tested (the /me field + the script's presence and path gate;
  behavior smoke via the rendered page containing the hook).
- **AC2 — Three complete skins.** `base.css` defines all three `[data-theme]` blocks covering:
  background/panel/border/accent tokens, headers, tables, nav highlight, links, and the status
  strip — visually distinct at a glance, readable (contrast preserved), Romans in the sepia
  direction. A static test pins the presence of the three blocks and a representative token in
  each.
- **AC3 — Signature buttons.** Primary action buttons (the existing primary-button class) get a
  per-tribe hover flourish (cape swing / forge ember / leaf unfurl), CSS-only, reduced-motion
  gated. Non-themed pages keep the neutral button.
- **AC4 — Ornamented headers + full-skin touches.** Panel/section headers carry the tribe's
  ornament detail; at least two further skin touches per tribe (e.g. table row hover tint,
  countdown accent, nav underline) — bounded list in the plan, so review has a checklist.
- **AC5 — No layout change, no regression.** No template structural changes beyond attributes/
  classes; the full web test suite stays green; pages without the attribute render byte-identical
  CSS-wise (the neutral default block is untouched).
- **AC6 — Reduced motion & performance.** All animations sit behind `prefers-reduced-motion`;
  only compositor-friendly properties animate (transform/opacity/filter); no JS animation loops.

## Roles & permissions

Per [roles.md](../../roles.md): cosmetic only; no role changes. Spectators/moderators browsing
game-adjacent pages see neutral chrome unless they themselves act as a player in a world.

## Out of scope

- New binary art / illustrated backgrounds beyond the existing plates.
- Theming public surfaces, the admin console, `/spectate`, `/manual`, `/docs/api`.
- A user setting to opt out per account (a follow-up if wanted; reduced-motion is respected now).
- Tribe-themed emails/notifications (none exist).
