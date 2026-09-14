## Design: fix-db8e-oled-placeholder

No design required — restoration of `db8e-oled-display-placeholder` parity.
The `B-grid` top is derived from `module.cells["B"]` min `y_mm`, band height
is `module_rect.height * (b_top_mm / module.h_mm)` rounded, and the state
text comes from `physical::db8e_display_state_for_layout` (existing logic in
`src/physical.rs`). Nothing else changes.
