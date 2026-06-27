# Tasks — 096 map "Send merchant"

Branch `feature/096-map-send-merchant`.

- [x] **T1**: market handler accepts `Query<MapQuery>` → `MarketTemplate.target_x/target_y`; market.html
  pre-fills the target inputs (mirror rally).
- [x] **T2**: `MapCellView.market_href` (Serialize); `map_cells` sets it for any village tile.
- [x] **T3**: map.html — `#minspect-merchant` button; `select()` shows it when `data-market` present; render
  copies `market_href` to the tile dataset; server grid emits `data-market`.
- [x] **T4 — Verify**: live — village shows both buttons, oasis only troops; merchant link → Marketplace
  pre-filled. Tests: `/map/tiles` asserts village `market_href` + oasis null; market flow test still passes.
- [ ] **T5 — Gate + reviewer + PR.**
