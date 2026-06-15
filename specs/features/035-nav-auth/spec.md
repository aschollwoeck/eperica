# Feature 035 — Auth-aware navigation

**Status:** Draft
**Depends on:** the global topbar (`base.html`), auth (`AuthUser`/sitting 030), the Moderator role (022)
**Roadmap:** app-layer UX — a navigation correctness fix surfaced during the UX review.

## Problem

The topbar in `base.html` rendered a **fixed** set of links on every page, with no auth or role check
(no template carried nav state). Consequences:

- A **logged-out visitor** (landing, `/login`, `/register`) saw member-only links — Profile, Settings,
  Sitting, Messages, Notifications — all of which just bounce to `/login`.
- There was **no "Village" home link and no "Log out"** in the topbar; logout existed only as a button at
  the bottom of the village page, so from most pages you had to go "← Back to the village" first.
- The **moderator review queue (`/mod`) had no link anywhere** — reachable only by typing the URL.

## Goal

The topbar shows the link set appropriate to the viewer, without threading auth state through all ~60
page-template structs (P5: the web tier stays stateless; pages stay pure functions of their own data).

- **AC1 — Member vs visitor links.** When logged in, the topbar shows the member group (Village,
  Notifications, Messages, Profile, Settings, Sitting, Log out) and not the visitor group; when logged
  out, it shows the visitor group (Log in, Register) and not the member group. Public links (Leaderboards,
  Wonder, Search) show to everyone.
- **AC2 — Moderation link.** The Moderation link (`/mod`) appears only when the viewer holds the Moderator
  role (022) — matching the server-side gate, so the link appears exactly when the page would not 403.
- **AC3 — No wrong links shown.** A visitor never sees member-only links and vice versa (the groups
  default hidden and are revealed to the matching audience).

## Design

- **`GET /me` nav probe.** A best-effort, visitor-reachable endpoint returns
  `{"authed": bool, "moderator": bool}`. `authed` is true when the auth cookie resolves; `moderator`
  reflects the **effective** player's `is_moderator` (the same identity the 022 gate keys on, including
  while sitting). A new `MaybeAuthUser` extractor yields `Option<PlayerId>` and never rejects. Excluded
  from the 025 presence-touch (it fires on every page like the other background polls).
- **JS toggle in `base.html`.** The nav groups (`[data-auth="in"]`, `[data-auth="out"]`, `#nav-mod`)
  default `hidden`; on load the script fetches `/me` and reveals the matching group(s). This mirrors the
  topbar's existing JS-populated pieces (the unread/notification badges and the sitting banner) and keeps
  every page template free of auth state.
- **Why client-side.** Askama has no global template context; auth-aware server rendering would require a
  nav field on all ~60 structs. The topbar is already JS-enhanced, so a tiny probe + toggle is consistent
  and isolated, at the cost of revealing the correct group one fetch after first paint (public links and
  the page body render immediately).

## Out of scope

- Removing the village-page Log out button (kept; harmless duplicate).
- Visual theming of the topbar (the later styling pass).
- Server-side gating of the pages themselves (already enforced by extractors / use-case role checks — P4);
  this slice only governs link **visibility**.
