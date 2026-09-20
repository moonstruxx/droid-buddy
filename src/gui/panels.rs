//! Controller panels surface (task 2.2): the "Panels" pane ported from
//! `ui.rs::render_panels_pane`.
//!
//! In the terminal the pane was a bordered container (" Panels ") holding the
//! physical-full render; the egui port keeps that shape: [`paint_panels`]
//! draws the focused pane border and title, then the module sub-blocks
//! (faceplates grouped by circuit instance, `rendermetrics::PanelModel`
//! grouping) and their component cells. The cell rendering itself is shared
//! with the physical surface (`physical::paint_cell` / [`physical::CellSpec`]
//! / [`physical::rack_geometry`]) so both surfaces stay pixel-consistent;
//! what this module adds is the pane chrome, the shift-group borders, and the
//! hover/click/scroll input report ([`PanelsFrame`]).
//!
//! Tests build [`PanelsSpec`] directly. Colors always flow through
//! `crate::theme::active()`.

use std::collections::{HashMap, HashSet};

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::app::App;
use crate::patch::{ComponentKind, HwComponent, Patch};
use crate::theme::Color;

use super::physical::{cell_visuals, paint_cell, CellSpec, ModuleSpec, PortMark};

/// The resolved panels-pane payload for one frame: pane chrome state plus the
/// sub-blocks and cells to draw (pure data, like the other surface specs).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PanelsSpec {
    /// Pane title (the terminal's " Panels ").
    pub title: String,
    /// Keyboard focus state: the focused pane draws a bold `focus_border`
    /// instead of the muted border.
    pub focused: bool,
    /// Module sub-blocks (faceplates) with their outline rects and titles.
    pub modules: Vec<ModuleSpec>,
    /// Component cells inside the sub-blocks, in chain order. A cell with a
    /// `shift_color` (shift-group-active button) additionally draws a border
    /// in its group token.
    pub cells: Vec<CellSpec>,
    /// Global processing pause: everything renders dimmed.
    pub paused: bool,
}

/// The frame's panels input report, applied by the loop to `App` (terminal
/// semantics: hover sets `hovered_component`, a primary click selects, the
/// wheel over a knob/encoder adjusts its value).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PanelsFrame {
    /// Component index (into `patch.hw_components`) under the pointer.
    pub hovered: Option<usize>,
    /// Component index primary-clicked this frame.
    pub clicked: Option<usize>,
    /// Wheel delta over a knob/encoder cell (the value-adjust gesture).
    pub scroll: Option<f32>,
}

/// Paint the panels pane. `None` draws nothing. Returns the frame's
/// hover/click/scroll report for the loop to apply.
pub(super) fn paint_panels(
    ui: &mut egui::Ui,
    pane: egui::Rect,
    spec: Option<&PanelsSpec>,
) -> PanelsFrame {
    let Some(spec) = spec else {
        return PanelsFrame::default();
    };
    let painter = ui.painter().with_clip_rect(pane);
    let t = crate::theme::active();
    let border_color = if spec.focused {
        rgb(t.focus_border)
    } else {
        rgb(t.muted)
    };
    painter.rect(
        pane,
        0.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(if spec.focused { 2.0 } else { 1.0 }, border_color),
        egui::StrokeKind::Inside,
    );
    if !spec.title.is_empty() {
        painter.text(
            pane.min + egui::vec2(6.0, 3.0),
            egui::Align2::LEFT_TOP,
            &spec.title,
            egui::FontId::proportional(12.0),
            border_color,
        );
    }

    // Module sub-blocks: faceplate borders + titles, inset inside the pane.
    let inner = pane.shrink(3.0);
    let outline = rgb(t.physical_skeleton_module_outline);
    let title_color = rgb(t.text);
    for module in &spec.modules {
        let rect = module.rect.intersect(inner);
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        painter.rect(
            rect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, outline),
            egui::StrokeKind::Inside,
        );
        if !module.title.is_empty() && rect.height() >= 12.0 {
            painter.text(
                rect.min + egui::vec2(4.0, 2.0),
                egui::Align2::LEFT_TOP,
                &module.title,
                egui::FontId::proportional(11.0),
                title_color,
            );
        }
    }

    // Component cells: a shift-group border first (group token), then the
    // shared cell renderer (glyph/label/state or fader face, hover backdrop,
    // paused dim).
    for cell in &spec.cells {
        if let Some(shift) = cell.shift_color {
            painter.rect(
                cell.rect,
                0.0,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(1.5, rgb(shift)),
                egui::StrokeKind::Inside,
            );
        }
        paint_cell(&painter, cell, false, spec.paused);

        // AccessKit annotation for egui_kittest query-by-label
        let response = ui.allocate_rect(cell.rect, egui::Sense::hover());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &cell.label));
    }

    drop(painter);
    ui.ctx().input(|i| panels_frame(i, &spec.cells))
}

/// Cell size + gap of the quad Panels-pane wrap grid, in points. The pane is
/// a quadrant of the window, so cells stay compact: glyph + state + label fit
/// a 2-line cell like the physical surface's text cells.
const PANEL_CELL_W: f32 = 96.0;
const PANEL_CELL_H: f32 = 40.0;
const PANEL_CELL_GAP: f32 = 6.0;
/// Title-zone height above a module block's cell rows.
const PANEL_TITLE_H: f32 = 20.0;

/// The quad Panels-pane payload for one frame: every patch component grouped
/// by controller into titled faceplate blocks, flowing left-to-right and
/// wrapping rows within `pane`. `global_index` stays the component's index
/// into `patch.hw_components` so hit-testing matches the other surfaces.
pub(super) fn panels_spec(app: &App, focused: bool, pane: Rect) -> PanelsSpec {
    let t = crate::theme::active();
    let Some(patch) = app.patch.as_ref() else {
        return PanelsSpec {
            title: " Panels ".into(),
            focused,
            modules: Vec::new(),
            cells: Vec::new(),
            paused: app.processing_paused,
        };
    };
    let inner = pane.shrink(4.0);
    let mut modules = Vec::new();
    let mut cells = Vec::new();

    // HW-cell labels resolve through the overlay fallback chain
    // (`Patch::display_label`: store[layer] → store[1] → preamble[1] →
    // derived), so an edited label renders on the panels surface. The store,
    // shift layer, and layer config all come from App state: the `[labels]`
    // section is seeded at startup (ADR 13: config loads once), so painters
    // never re-read the config file per frame.
    let hw_store = app.current_hw_store();
    let shift = app.active_shift_layer();
    let layers_enabled = app.labels.layers_enabled;
    let max_shift_layer = app.labels.max_shift_layer;
    let label_for = |patch: &Patch, comp: &HwComponent| {
        patch.display_label(&comp.id, shift, layers_enabled, max_shift_layer, &hw_store)
    };
    // Modifier wash set (design D): while a modifier is active (latched or
    // held), its influenced hardware tokens wash in the token hue and every
    // other cell dims. The set is computed once per frame.
    let modifier = app.active_modifier();
    let wash_tokens: HashSet<String> = match (modifier, app.modifier_influence.as_ref()) {
        (Some(_tok), Some(inf)) => patch.influenced_hw_tokens(inf).into_iter().collect(),
        _ => HashSet::new(),
    };

    // Group by controller, first-seen declaration order.
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, Vec<(usize, &HwComponent)>> = HashMap::new();
    for (gi, comp) in patch.hw_components.iter().enumerate() {
        if !groups.contains_key(comp.controller.as_str()) {
            order.push(comp.controller.as_str());
        }
        groups
            .entry(comp.controller.as_str())
            .or_default()
            .push((gi, comp));
    }

    // Flow module blocks horizontally, wrapping rows.
    let mut x = inner.min.x;
    let mut y = inner.min.y;
    let row_height = PANEL_CELL_H + PANEL_CELL_GAP;
    for controller in order {
        let comps = &groups[controller];
        // All cells of this module go in one row (block_cols = comps.len); the outer
        // wrap handles pane overflow at the module level.
        let block_cols = comps.len();
        let rows = 1;
        let block_h = PANEL_TITLE_H + rows as f32 * row_height;
        let block_w = block_cols as f32 * (PANEL_CELL_W + PANEL_CELL_GAP) - PANEL_CELL_GAP;

        // Wrap to next row if this block does not fit in the remaining width.
        if x + block_w > inner.max.x + 0.5 && x > inner.min.x {
            x = inner.min.x;
            y += block_h + PANEL_CELL_GAP;
        }

        let block_x = x;
        modules.push(ModuleSpec {
            rect: Rect::from_min_size(Pos2::new(block_x, y), Vec2::new(block_w, block_h)),
            title: controller.to_string(),
        });

        for (slot, (gi, comp)) in comps.iter().enumerate() {
            // All cells share row 0 within this module.
            let col = slot;
            let cx = block_x + col as f32 * (PANEL_CELL_W + PANEL_CELL_GAP);
            let cy = y + PANEL_TITLE_H;
            let is_shift_active =
                comp.shift_group.is_some() && comp.shift_group == app.active_shift;
            let (glyph, state_text, color) = cell_visuals(comp, is_shift_active, false);
            cells.push(CellSpec {
                rect: Rect::from_min_size(Pos2::new(cx, cy), Vec2::new(PANEL_CELL_W, PANEL_CELL_H)),
                glyph,
                label: label_for(patch, comp),
                state_text,
                color,
                is_fader: false,
                fader_value: 0.0,
                global_index: *gi,
                mark: PortMark::Cell,
                highlighted: app.selected_component.as_deref() == Some(comp.id.as_str()),
                shift_color: if is_shift_active {
                    comp.shift_group.map(|g| match g {
                        crate::patch::ShiftGroup::Group1 => t.shift1,
                        crate::patch::ShiftGroup::Group2 => t.shift2,
                        crate::patch::ShiftGroup::Group3 => t.shift3,
                        crate::patch::ShiftGroup::Group4 => t.shift4,
                    })
                } else {
                    None
                },
                kind: comp.kind,
                modifier_wash: modifier.and_then(|tok| {
                    wash_tokens
                        .contains(comp.id.as_str())
                        .then(|| crate::theme::modifier_hue(tok))
                }),
                dimmed: modifier.is_some() && !wash_tokens.contains(comp.id.as_str()),
            });
        }
        x += block_w + PANEL_CELL_GAP;
    }
    PanelsSpec {
        title: " Panels ".into(),
        focused,
        modules,
        cells,
        paused: app.processing_paused,
    }
}

/// The pane's hover/click/scroll report from the egui input state: the cell
/// under the pointer, a primary click on it, and the wheel delta when the
/// pointer is over a knob/encoder (the value-adjust gesture). Pure so input
/// tests run without a window.
pub(super) fn panels_frame(i: &egui::InputState, cells: &[CellSpec]) -> PanelsFrame {
    let mut frame = PanelsFrame::default();
    let Some(pos) = i.pointer.latest_pos() else {
        return frame;
    };
    frame.hovered = hit_test(cells, pos);
    if i.pointer.primary_clicked() {
        frame.clicked = frame.hovered;
    }
    if i.smooth_scroll_delta.y.abs() > f32::EPSILON {
        if let Some(h) = frame.hovered {
            if matches!(cells[h].kind, ComponentKind::Knob | ComponentKind::Encoder) {
                frame.scroll = Some(i.smooth_scroll_delta.y);
            }
        }
    }
    frame
}

/// The first cell whose rect contains `pos` (last drawn wins, matching the
/// terminal hit-testing contract of `component_rects`).
fn hit_test(cells: &[CellSpec], pos: egui::Pos2) -> Option<usize> {
    cells.iter().rposition(|c| c.rect.contains(pos))
}
fn rgb(color: Color) -> egui::Color32 {
    crate::theme::active().egui_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::physical::PortMark;
    use crate::patch::ComponentKind;
    use std::path::Path;

    fn cell(
        rect: Rect,
        glyph: &str,
        label: &str,
        state: &str,
        kind: ComponentKind,
        shift: Option<Color>,
    ) -> CellSpec {
        CellSpec {
            rect,
            glyph: glyph.into(),
            label: label.into(),
            state_text: state.into(),
            color: crate::theme::active().muted,
            is_fader: false,
            fader_value: 0.0,
            global_index: 0,
            mark: PortMark::Cell,
            highlighted: false,
            shift_color: shift,
            modifier_wash: None,
            dimmed: false,
            kind,
        }
    }

    /// A shift-active button and an LED on one module sub-block.
    fn spec() -> PanelsSpec {
        let button = cell(
            Rect::from_min_size(Pos2::new(6.0, 6.0), Vec2::new(40.0, 30.0)),
            "\u{25CF}",
            "B1.1",
            "ON",
            ComponentKind::Button,
            Some(crate::theme::active().shift2),
        );
        let led = cell(
            Rect::from_min_size(Pos2::new(50.0, 6.0), Vec2::new(40.0, 30.0)),
            "\u{25CB}",
            "L1.1",
            "OFF",
            ComponentKind::Led,
            None,
        );
        PanelsSpec {
            title: " Panels ".into(),
            focused: true,
            modules: vec![ModuleSpec {
                rect: Rect::from_min_size(Pos2::new(2.0, 2.0), Vec2::new(96.0, 38.0)),
                title: "P2B8 1".into(),
            }],
            cells: vec![button, led],
            paused: false,
        }
    }

    /// Paint the spec, returning (labels, rect shapes, text shapes).
    fn painted(
        spec: &PanelsSpec,
    ) -> (
        Vec<String>,
        Vec<egui::epaint::RectShape>,
        Vec<egui::epaint::TextShape>,
    ) {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(200.0, 60.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_panels(ui, ui.max_rect(), Some(spec));
        });
        let mut labels = Vec::new();
        let mut rects = Vec::new();
        let mut texts = Vec::new();
        for cs in &full_output.shapes {
            match &cs.shape {
                egui::epaint::Shape::Text(t) => {
                    labels.push(t.galley.text().to_string());
                    texts.push(t.clone());
                }
                egui::epaint::Shape::Rect(r) => rects.push(r.clone()),
                _ => {}
            }
        }
        full_output.textures_delta.clear();
        (labels, rects, texts)
    }

    #[test]
    fn pane_draws_focused_border_and_title() {
        let (labels, rects, _) = painted(&spec());
        assert!(
            labels.iter().any(|l| l.contains("Panels")),
            "pane title: {labels:?}"
        );
        let focus = crate::theme::active().egui_color(crate::theme::active().focus_border);
        assert!(
            rects.iter().any(
                |r| r.rect == Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 60.0))
                    && r.stroke.color == focus
            ),
            "focused pane border drawn: {rects:?}"
        );
    }

    #[test]
    fn module_sub_blocks_render_outline_and_title() {
        let (labels, rects, _) = painted(&spec());
        assert!(
            labels.iter().any(|l| l.contains("P2B8 1")),
            "sub-block title: {labels:?}"
        );
        assert!(
            rects
                .iter()
                .any(|r| r.rect == Rect::from_min_max(Pos2::new(3.0, 3.0), Pos2::new(98.0, 40.0))),
            "sub-block outline: {rects:?}"
        );
    }

    #[test]
    fn shift_border_and_led_cell_render() {
        let (labels, rects, _) = painted(&spec());
        // LED cell: hollow glyph + OFF state drawn.
        assert!(
            labels.iter().any(|l| l.contains("\u{25CB}")),
            "led glyph: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("OFF")),
            "led state: {labels:?}"
        );
        // The shift-active button carries a border in its group token.
        let shift2 = crate::theme::active().egui_color(crate::theme::active().shift2);
        assert!(
            rects.iter().any(|r| r.rect
                == Rect::from_min_size(Pos2::new(6.0, 6.0), Vec2::new(40.0, 30.0))
                && r.stroke.color == shift2),
            "shift border on the active-group button: {rects:?}"
        );
    }

    #[test]
    fn panels_frame_reports_hover_click_and_knob_scroll() {
        let s = spec();
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(200.0, 60.0),
            )),
            events: vec![
                egui::Event::PointerMoved(Pos2::new(70.0, 21.0)), // over the LED cell
                egui::Event::PointerButton {
                    pos: Pos2::new(70.0, 21.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerButton {
                    pos: Pos2::new(70.0, 21.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -30.0),
                    modifiers: egui::Modifiers::default(),
                    phase: egui::TouchPhase::Move,
                },
            ],
            ..Default::default()
        };
        let mut frame = PanelsFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = ui.ctx().input(|i| panels_frame(i, &s.cells));
        });
        full_output.textures_delta.clear();
        assert_eq!(frame.hovered, Some(1), "LED cell under the pointer");
        assert_eq!(frame.clicked, Some(1));
        assert_eq!(frame.scroll, None, "the LED is not a knob: no value scroll");

        // Over a knob cell the wheel reports the value scroll.
        let knob = cell(
            Rect::from_min_size(Pos2::new(120.0, 6.0), Vec2::new(40.0, 30.0)),
            "\u{25C9}",
            "P1.1",
            "50%",
            ComponentKind::Knob,
            None,
        );
        let mut cells = s.cells.clone();
        cells.push(knob);
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(200.0, 60.0),
            )),
            events: vec![
                egui::Event::PointerMoved(Pos2::new(140.0, 21.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -30.0),
                    modifiers: egui::Modifiers::default(),
                    phase: egui::TouchPhase::Move,
                },
            ],
            ..Default::default()
        };
        let mut frame = PanelsFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = ui.ctx().input(|i| panels_frame(i, &cells));
        });
        full_output.textures_delta.clear();
        assert_eq!(frame.hovered, Some(2), "knob cell under the pointer");
        assert!(
            frame.scroll.is_some_and(|d| d < 0.0),
            "wheel over a knob reports a downward value scroll: {:?}",
            frame.scroll
        );
    }

    #[test]
    fn paint_none_spec_draws_nothing() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let mut frame = PanelsFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = paint_panels(ui, ui.max_rect(), None);
        });
        full_output.textures_delta.clear();
        assert_eq!(frame, PanelsFrame::default());
        assert!(full_output.shapes.is_empty());
    }

    #[test]
    fn panels_spec_groups_every_component_by_controller() {
        let mut app = crate::app::App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        // A wide pane so every module block (a module is one row of cells) fits
        // horizontally; the horizontal layout wraps at the block level only.
        let pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(3000.0, 800.0));
        let spec = panels_spec(&app, true, pane);
        assert!(spec.focused);
        let patch = app.patch.as_ref().unwrap();
        // Every component appears exactly once, keeping its hw_components
        // index for hit-testing.
        assert_eq!(spec.cells.len(), patch.hw_components.len());
        for (i, c) in spec.cells.iter().enumerate() {
            assert_eq!(c.global_index, i);
        }
        // One titled block per controller, and cells stay inside the pane.
        let controllers: std::collections::HashSet<&str> = patch
            .hw_components
            .iter()
            .map(|c| c.controller.as_str())
            .collect();
        assert_eq!(spec.modules.len(), controllers.len());
        for m in &spec.modules {
            assert!(controllers.contains(m.title.as_str()));
        }
        // Wrapping keeps cells within the pane horizontally; rows can extend
        // past the pane's bottom edge (the pane scrolls), so only the x bound
        // is asserted here.
        assert!(
            spec.cells.iter().all(|c| c.rect.max.x <= pane.max.x + 0.5),
            "wrapped cells must stay inside the pane horizontally"
        );
    }

    #[test]
    fn panels_spec_highlights_selected_and_shift_active_cells() {
        let mut app = crate::app::App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        let first = app.patch.as_ref().unwrap().hw_components[0].id.clone();
        app.selected_component = Some(first.clone());
        let pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(600.0, 400.0));
        let spec = panels_spec(&app, false, pane);
        let selected = spec
            .cells
            .iter()
            .find(|c| c.global_index == 0)
            .expect("first component cell");
        assert!(selected.highlighted);
        let other = spec.cells.iter().find(|c| c.global_index == 1);
        if let Some(other) = other {
            assert!(!other.highlighted);
        }
    }

    #[test]
    fn panels_spec_empty_without_patch() {
        let app = crate::app::App::new();
        let pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(600.0, 400.0));
        let spec = panels_spec(&app, false, pane);
        assert!(!spec.focused);
        assert!(spec.modules.is_empty());
        assert!(spec.cells.is_empty());
    }

    #[test]
    fn panels_spec_flows_horizontally() {
        // Module blocks always flow left-to-right and wrap rows; there is no
        // portrait vertical-stack variant anymore.
        let mut app = crate::app::App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        let cell_count = patch.hw_components.len();
        assert!(app.load_patch(patch));

        let narrow_pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(300.0, 800.0));
        let spec = panels_spec(&app, false, narrow_pane);
        assert_eq!(spec.cells.len(), cell_count);

        // Cells within a module share the same y: they flow horizontally.
        for module in &spec.modules {
            let module_cells: Vec<_> = spec
                .cells
                .iter()
                .filter(|c| module.rect.contains_rect(c.rect))
                .collect();
            if module_cells.len() > 1 {
                let mut cell_ys: Vec<f32> = module_cells.iter().map(|c| c.rect.min.y).collect();
                cell_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
                cell_ys.dedup();
                assert_eq!(cell_ys.len(), 1, "module cells should share the same y");
            }
        }

        // The narrow pane forces wrapping onto more than one module row.
        let mut module_ys: Vec<f32> = spec.modules.iter().map(|m| m.rect.min.y).collect();
        module_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        module_ys.dedup();
        assert!(
            module_ys.len() > 1,
            "narrow pane should wrap modules onto more than one row"
        );

        // Module blocks start at the pane's left edge (left-to-right flow).
        let inner = narrow_pane.shrink(4.0);
        let first_row_min_y = module_ys[0];
        let first_row: Vec<_> = spec
            .modules
            .iter()
            .filter(|m| m.rect.min.y == first_row_min_y)
            .collect();
        assert_eq!(
            first_row[0].rect.min.x, inner.min.x,
            "first block in a row should start at the pane's left edge"
        );
    }
}
