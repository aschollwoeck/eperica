# Feature 090 — the building art is a page-wide backdrop, not a top band

## Why

The building pages showed their art in a 320px hero **band** that pushed all content down and then stopped
abruptly (faded to the page colour at the band's bottom). The art wasted vertical space and didn't extend
behind the content. Make it a page-wide backdrop the cards sit over, and shrink the hero so the content
starts higher (less scrolling).

Presentation only — CSS; no template/handler change (P3). The `--building-img` / `--building-img-tribe` vars
are still set per-page in each building template's `body_style` (063/065) — this only changes where they're
consumed.

## Acceptance criteria

- **AC1 — Backdrop behind the content.** The building art renders as a fixed, page-wide backdrop
  (`.bld-page main::before`) behind the whole page — the hero, ribbon, and cards sit over it and it shows in
  the gaps. It fades to the page colour lower down so content stays readable, and (being fixed) it stays
  behind the page through the footer rather than stopping at a band.
- **AC2 — Less scrolling.** The hero shrinks (≈320→190px desktop), so the cards start ~130px higher and the
  page is shorter (smithy ≈1219→1089px).
- **AC3 — No regression.** No horizontal overflow desktop/mobile. Other `.bld-page` views that set no
  `--building-img` (village/map/admin/…) get an inert dark wash (no visible image), unchanged.

## Out of scope

- Per-card translucency (the cards stay opaque for contrast; the art shows around/behind them).
