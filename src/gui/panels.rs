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

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::patch::ComponentKind;
use crate::theme::Color;

use super::physical::{paint_cell, CellSpec, ModuleSpec};

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
    painter: &Painter,
    pane: egui::Rect,
    ctx: &Context,
    spec: Option<&PanelsSpec>,
) -> PanelsFrame {
    let Some(spec) = spec else {
        return PanelsFrame::default();
    };
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
        paint_cell(painter, cell, false, spec.paused);
    }

    ctx.input(|i| panels_frame(i, &spec.cells))
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
            paint_panels(ui.painter(), ui.max_rect(), ui.ctx(), Some(spec));
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
            frame = paint_panels(ui.painter(), ui.max_rect(), ui.ctx(), None);
        });
        full_output.textures_delta.clear();
        assert_eq!(frame, PanelsFrame::default());
        assert!(full_output.shapes.is_empty());
    }
}
