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
3. **Medieval, but restrained.** A parchment/wood/iron/gold palette evokes the setting without heavy
   skeuomorphism that would cost performance or hurt readability.
4. **Server-authoritative, client-smooth (P1/P4/P11).** Countdowns and resource counters tick on the
   client from server-provided timestamps; the server remains the source of truth.
5. **Accessible by default.** WCAG AA contrast, semantic HTML, keyboard navigability, reduced-motion
   support — not optional (P10).
6. **Consistency via tokens & components.** No ad-hoc colors or spacing; everything references a token.

---

## 2. Design tokens

Declared once as CSS custom properties in `:root`. Never hardcode raw values in components.

### 2.1 Color — surfaces & ink
| Token | Hex | Use |
|-------|-----|-----|
| `--c-parchment` | `#F4ECD8` | App background |
| `--c-panel` | `#FBF6E9` | Cards/panels |
| `--c-panel-alt` | `#E8DCC0` | Subtle panel/zebra rows |
| `--c-ink` | `#2B2117` | Primary text |
| `--c-ink-muted` | `#5A4D3B` | Secondary text |
| `--c-border` | `#C9B68C` | Borders/dividers |

### 2.2 Color — brand & structural
| Token | Hex | Use |
|-------|-----|-----|
| `--c-wood` | `#6B4F2E` | Primary structural/brand brown |
| `--c-wood-dark` | `#4A3620` | Headers, footers, nav |
| `--c-iron` | `#3F4045` | Strong contrast surfaces |
| `--c-gold` | `#C9A227` | Accents, medals, highlights |

### 2.3 Color — resources (CANONICAL — identical everywhere a resource appears)
| Token | Hex | Resource |
|-------|-----|----------|
| `--c-lumber` | `#3F7D3F` | Wood (lumber) |
| `--c-clay` | `#B5652E` | Clay |
| `--c-iron-res` | `#6C7A89` | Iron |
| `--c-crop` | `#D8A92B` | Crop |

### 2.4 Color — semantic / state
| Token | Hex | Meaning |
|-------|-----|---------|
| `--c-danger` | `#B33A3A` | Incoming attack, destructive actions, errors |
| `--c-success` | `#2E7D4F` | Successful defense, confirmations |
| `--c-warning` | `#C9772B` | Low/negative crop, cautions |
| `--c-info` | `#3A6EA5` | Neutral information |

All text/background pairings must meet **WCAG AA (4.5:1)**; large text/UI elements **3:1** minimum.

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
- **One stylesheet**, organized in layers and concatenated/served as a single cacheable file:
  `tokens.css → base.css (reset + element defaults) → layout.css → components.css → utilities.css`.
- **Location:** static assets in the `web` crate (e.g. `web/static/css/`), served with long cache
  headers + content hashing.
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
