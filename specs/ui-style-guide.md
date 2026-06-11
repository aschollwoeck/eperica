# Eperica — UI Style Guide & Design System

**Status:** Standing document v1
**Governed by:** [constitution.md](./constitution.md) — esp. **P11** (performance) and **P10**
(portfolio-grade).

This is the **visual and front-end standard** every web slice must conform to. Its plan's "Interface"
section must reference and obey this guide. It defines the design language (tokens, components) and the
front-end conventions (CSS/Askama/htmx) for the Eperica web client.

> **Stack reminder:** server-rendered **Askama** HTML + **htmx** for interactivity + a tiny bit of
> vanilla JS (live countdowns). **No CSS framework** and **no SPA** — hand-rolled CSS with design
> tokens, for control and speed (P11).

---

## 1. Design principles

1. **Performance-first (P11).** One small, cacheable stylesheet; system fonts (no render-blocking web
   fonts); inline SVG icons; minimal JS. The UI must never be the latency bottleneck.
2. **Legibility under density.** It's a numbers game (resources, timers, troop counts). Optimize for
   scannability: tabular figures, clear hierarchy, generous alignment.
3. **Grim medieval, dark and warm.** A dark, near-black canvas lit by warm rust, bronze, and gold —
   atmospheric and fire-lit, never the bright heraldic look — while staying legible and free of heavy
   skeuomorphism. (Canonical theme: **"Ash & Rust"**.)
4. **Server-authoritative, client-smooth (P1/P4/P11).** Countdowns and resource counters tick on the
   client from server-provided timestamps; the server remains the source of truth.
5. **Accessible by default.** WCAG AA contrast, semantic HTML, keyboard navigability, reduced-motion
   support — not optional (P10).
6. **Consistency via tokens & components.** No ad-hoc colors or spacing; everything references a token.

---

## 2. Design tokens

Declared once as CSS custom properties in `:root`. Never hardcode raw values in components.

Theme-agnostic structure lives in `base.css`; these color tokens are defined by the active theme file
(`static/theme-ash.css`, canonical).

### 2.1 Color — surfaces & ink
| Token | Hex | Use |
|-------|-----|-----|
| `--c-bg` | `#141110` | App background (near-black, warm) |
| `--c-panel` | `#1D1714` | Cards/panels |
| `--c-panel-alt` | `#261E19` | Raised/zebra surfaces, inputs, top bar |
| `--c-ink` | `#E0D3C1` | Primary text (warm parchment) |
| `--c-ink-muted` | `#9C8B78` | Secondary text |
| `--c-border` | `#41342A` | Borders/dividers |

### 2.2 Color — brand & accent
| Token | Hex | Use |
|-------|-----|-----|
| `--c-accent` | `#C0623A` | Primary accent — buttons, active states (rust/copper) |
| `--c-accent-ink` | `#141110` | Text/icons on accent surfaces |
| `--c-gold` | `#C79350` | Brand wordmark, links, medals, highlights (bronze-gold) |

### 2.3 Color — resources (CANONICAL — identical everywhere a resource appears)
| Token | Hex | Resource |
|-------|-----|----------|
| `--c-lumber` | `#98B266` | Wood (lumber) |
| `--c-clay` | `#CE7A48` | Clay |
| `--c-iron-res` | `#ABA08C` | Iron |
| `--c-crop` | `#DDB14A` | Crop |

### 2.4 Color — semantic / state
| Token | Hex | Meaning |
|-------|-----|---------|
| `--c-danger` | `#CB5A4C` | Incoming attack, destructive actions, errors |
| `--c-success` | `#93AC5C` | Successful defense, confirmations |
| `--c-warning` | `#D69640` | Low/negative crop, cautions |

Resource and semantic colors are tuned to read clearly on the dark canvas. Text/background pairings
must meet **WCAG AA (4.5:1)**; large text/UI elements **3:1** minimum. Translucent fills (e.g. alert
backgrounds) are derived with `color-mix(in srgb, var(--c-…) 14%, transparent)` rather than new tokens.

### 2.5 Typography
- **Body:** system stack — `-apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif`.
- **Headings:** serif system stack — `Georgia, "Times New Roman", serif` (medieval feel, zero font
  cost). An optional self-hosted display face (e.g. a Cinzel-like serif) may be added later **only** if
  self-hosted and subset (no FOUT/CLS).
- **Numbers (resources, timers, counts):** always `font-variant-numeric: tabular-nums` so values don't
  jitter as they tick.
- **Scale:** `--fs-xs .75rem`, `--fs-sm .875rem`, `--fs-base 1rem`, `--fs-lg 1.25rem`,
  `--fs-xl 1.5rem`, `--fs-2xl 2rem`. Base line-height 1.5.

### 2.6 Spacing, radius, elevation
- **Spacing (4px base):** `--space-1 4px` · `2 8px` · `3 12px` · `4 16px` · `5 24px` · `6 32px` ·
  `7 48px` · `8 64px`.
- **Radius:** `--radius-sm 3px`, `--radius-md 6px` (subtle; no pill/heavy rounding).
- **Border:** `1px solid var(--c-border)` default.
- **Elevation:** `--shadow-sm 0 1px 2px rgba(43,33,23,.12)`, `--shadow-md 0 2px 6px rgba(43,33,23,.16)`.

### 2.7 Breakpoints (desktop-first; strategy play is desktop-heavy)
- `sm` ≤640px (mobile) · `md` 641–1024px (tablet) · `lg` ≥1025px (desktop, primary target).

---

## 3. Layout — the app shell

```
┌───────────────────────────────────────────────┐
│ Resource bar  (always visible; per-resource:   │  ← --c-wood-dark bg
│  icon · current/capacity · +production/h)      │
├───────────┬───────────────────────────────────┤
│  Side nav │            Main content            │
│ (villages,│   (village view, map, reports…)    │
│  buildings)│                                   │
├───────────┴───────────────────────────────────┤
│ Footer (links, server/world info, speed)       │
└───────────────────────────────────────────────┘
```

- **Resource bar** is the signature persistent component: each resource shows icon + current/capacity
  (tabular-nums) + hourly production, color-coded by resource token; **crop turns `--c-warning`/
  `--c-danger`** when net production ≤ 0 (starvation risk).
- Pre-login pages (landing, register, login) use a simpler centered single-column layout (no shell).

---

## 4. Components (canonical set)

Each is a BEM block (§5). Specs are behavioral; exact CSS lives in the web crate.

- **Button** — `.btn` with `--btn--primary` (gold/wood), `--btn--secondary` (outline), `--btn--danger`
  (red). Clear focus ring; disabled state; never color-only signaling (include text/icon).
- **Panel / Card** — `.panel` on `--c-panel`, 1px border, `--radius-md`, optional header.
- **Form controls** — `.field` (label + input + help/error). Errors use `--c-danger` text **and** an
  icon/message (not color alone). Inputs ≥44px touch target on mobile.
- **Choice group** — `.choice` (a `<fieldset>` with a `<legend>`) of `.choice__option` radio labels,
  each an outlined card (`--c-border`, `--radius-md`) holding the radio, a `.choice__title`, and a
  one-line `.choice__desc` in `--c-ink-muted`; the checked card gets a `--c-gold` border. Used for
  mutually-exclusive picks with consequences (e.g. tribe selection at registration).
- **Resource bar** — `.resource-bar` / `.resource-bar__item` (see §3).
- **Countdown** — see §6; the live-timer primitive.
- **Progress bar** — `.progress` for build/training completion; pairs with a countdown.
- **Table** — `.table` for leaderboards/troop lists; zebra rows via `--c-panel-alt`; right-align and
  tabular-nums for numeric columns; sortable headers later.
- **Tabs** — `.tabs` for grouping (e.g. building categories).
- **Alert / toast** — `.alert--{danger,success,warning,info}`; **incoming attack = `--c-danger`** and
  must be unmissable.
- **Badge** — `.badge` for medals/achievements (gold), counts, statuses.
- **Modal / dialog** — `.modal` using the native `<dialog>` element (no JS framework); focus-trapped.
- **Tooltip** — `.tooltip` for stat explanations; never the only place critical info lives.

---

## 5. CSS conventions

- **Tokens only:** components reference `var(--…)`; no raw hex/px in component rules.
- **Methodology:** **BEM** — `.block`, `.block__element`, `.block--modifier`. Plus a tiny utility set
  (`.u-mt-3`, `.u-text-muted`, `.u-tabular`).
- **Two stylesheets:** `base.css` holds theme-agnostic structure (reset, layout, components,
  spacing/radius/shadow), all referencing `--c-*` tokens; the active **theme file**
  (`static/theme-ash.css`) defines those color tokens in `:root`. Pages load `base.css` + the theme.
  Swapping themes is just swapping the theme file.
- **Location:** `crates/web/static/`, served via `ServeDir` at `/static`. A living component gallery
  is available at **`/styleguide`**.
- **No CSS framework**, no runtime CSS-in-JS. Keep specificity low and flat.

---

## 6. Interactivity (htmx + minimal JS)

- **htmx** for partial updates (submit a build order → swap the queue panel) — avoids full reloads and
  keeps the server authoritative (P4). Show a request-in-flight indicator.
- **Live countdowns** are the one piece of bespoke JS:
  ```html
  <span class="countdown" data-deadline="2026-06-10T12:00:00Z">--:--:--</span>
  ```
  A small script ticks every second, renders `D:HH:MM:SS` (tabular-nums), and computes purely from the
  **server-provided UTC deadline** — never a client-started clock (P1/P4/P11). On reaching zero it may
  trigger an htmx refresh of the affected panel.
- **Resource counters** may extrapolate locally between requests from `{amount, rate, since}` provided
  by the server, re-syncing on each response.

---

## 7. Iconography

- **Inline SVG sprite** (`<svg><use href="#icon-…">`); no icon-font, no per-icon requests.
- Standard sizes 16 / 20 / 24px. Resource and building icons share a consistent visual weight.
- Icons that convey meaning carry `aria-label`/`<title>`; decorative icons are `aria-hidden`.

---

## 8. Accessibility & motion

- Semantic HTML5 landmarks (`header/nav/main/footer`), proper headings order, labelled controls.
- Visible `:focus-visible` styles on all interactive elements; full keyboard operability.
- Honor `prefers-reduced-motion` (no nonessential animation/transitions when set).
- Never signal state by color alone (pair with text/icon) — esp. resources and attack alerts.

---

## 9. How this guide is used

- Every **web slice's `plan.md`** cites this guide in its Interface section and lists any new
  components it introduces (which get added here).
- Slice **001** establishes the foundation: the token file, base/reset, the app-shell skeleton, and the
  form/button components needed for register/login/village views.
- Changes to tokens/components are made **here first**, then implemented (spec-before-behavior, P8).

## Changelog
- **v1 (2026-06-10)** — Initial design system: principles, tokens, layout, components, conventions.
- **v2 (2026-06-10)** — Adopt the dark, warm **"Ash & Rust"** palette (replacing the light
  parchment/heraldic palette); split CSS into `base.css` + theme files; add the `/styleguide` gallery.
