//! Physical 1:1 view surface (task 2.1): the mm-accurate rack presentation
//! ported from `ui.rs::render_physical_full` / `render_physical_skeleton` to
//! an egui draw routine.
//!
//! The surface follows the graph canvas pattern: a fully-resolved pure-data
//! payload ([`PhysicalSpec`]) built each frame, a draw routine
//! ([`paint_physical`]) that only paints it, and pure geometry helpers
//! ([`rack_geometry`], [`cell_visuals`]) that keep the mm→point math and the
//! per-kind cell styling testable headless. Input is reported back as a
//! [`PhysicalFrame`] (pan/zoom/skeleton) so the window loop applies the same
//! semantics the terminal handler used: `+`/`-` zoom presets, arrow/wheel
//! panning, and `s` skeleton toggle.
//!
//! The shell (main.rs / task 2.6) builds [`PhysicalSpec`] from `App` state:
//! `PhysicalLayout::build` + `RackLayout::pack` (pure, `src/physical.rs`),
//! the shared [`ScreenMapping`] (mm↔screen cells, also pure), and the display
//! labels from `Patch::display_label`. Tests construct specs directly.

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::patch::{ComponentKind, ComponentState, ShiftGroup};
use crate::physical::{PhysicalLayout, RackLayout, ScreenMapping};
use crate::theme::Color;

use super::graph::{MAX_ZOOM_STEP, ZOOM_SENSITIVITY};

/// What an element cell draws in skeleton mode: a plain cell dot or an
/// in/out port marker (CV jacks on the master faceplate), mirroring
/// `ui.rs::PortMark`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortMark {
    /// Plain cell dot.
    Cell,
    /// Input port marker (◀).
    PortIn,
    /// Output port marker (▶).
    PortOut,
}

/// A resolved module outline for painting.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModuleSpec {
    pub rect: Rect,
    pub title: String,
}

/// A resolved element cell for painting: the screen rect (points), the
/// glyph/state the kind renders, its theme color, and the interaction flags
/// (hover/circuit highlight, shift color, paused dim). `global_index` is the
/// component's index into `patch.hw_components`, so the window can hit-test
/// exactly like the terminal did.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellSpec {
    pub rect: Rect,
    pub glyph: String,
    pub label: String,
    pub state_text: String,
    pub color: Color,
    pub is_fader: bool,
    pub fader_value: f32,
    pub global_index: usize,
    pub mark: PortMark,
    pub highlighted: bool,
    pub shift_color: Option<Color>,
    /// Component kind: lets the input layer tell knobs/encoders apart for
    /// wheel-over-component semantics.
    pub kind: crate::patch::ComponentKind,
}

/// The fully-resolved physical-view payload for one frame. Pure data: the
/// shell builds it from `App` (mapping, labels, hover, pause state), the
/// tests build it directly; [`paint_physical`] only draws it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PhysicalSpec {
    pub background: Color,
    pub case_rect: Rect,
    pub mounts: Vec<Rect>,
    pub fold_bars: Vec<(Rect, String)>,
    pub modules: Vec<ModuleSpec>,
    pub cells: Vec<CellSpec>,
    /// Vertical + horizontal mm grid lines, already mapped to points.
    pub grid_lines: Vec<(Pos2, Pos2)>,
    /// Points per screen cell: the egui-layer scale over the shared
    /// `ScreenMapping` (mm→cells), used to convert pointer pan deltas back
    /// into cell units for `App::physical_offset`.
    pub cell_scale: f32,
    /// Skeleton presentation: module outlines + cell markers only.
    pub skeleton: bool,
    /// Global processing pause: everything renders dimmed.
    pub paused: bool,
}

/// The frame's physical-view input, reported back for the loop to apply to
/// `App` (mirrors `WindowFrame`): middle-drag pan in screen cells, wheel
/// zoom about the cursor, and a plain `s` skeleton toggle.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PhysicalFrame {
    /// Pan delta in screen cells (`physical_offset` units).
    pub pan_delta: (f32, f32),
    /// Wheel zoom: factor plus the cursor anchor in points.
    pub zoom: Option<(f32, (f32, f32))>,
    /// `s` pressed this frame (no modifiers): toggle skeleton presentation.
    pub skeleton_toggle: bool,
}

/// Paint the physical 1:1 view into the window canvas. `None` draws nothing.
/// Returns the frame's pan/zoom/skeleton input report for the loop to apply.
pub(crate) fn paint_physical(
    painter: &Painter,
    canvas: Vec2,
    ctx: &Context,
    spec: Option<&PhysicalSpec>,
) -> PhysicalFrame {
    let Some(spec) = spec else {
        return PhysicalFrame::default();
    };
    painter.rect_filled(
        Rect::from_min_size(Pos2::ZERO, canvas),
        0.0,
        rgb(spec.background),
    );

    let grid = rgb(crate::theme::active().muted);
    for (a, b) in &spec.grid_lines {
        painter.line_segment([*a, *b], egui::Stroke::new(1.0, grid.gamma_multiply(0.5)));
    }

    // Case outline wrapping all rows, then attached mount regions (empty for
    // the default case), then labeled fold bars at row boundaries.
    let case = rgb(crate::theme::active().graph_cluster_border);
    painter.rect(
        spec.case_rect,
        0.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, case),
        egui::StrokeKind::Inside,
    );
    for mount in &spec.mounts {
        painter.rect(
            *mount,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, case),
            egui::StrokeKind::Inside,
        );
    }
    let muted = rgb(crate::theme::active().muted);
    for (rect, label) in &spec.fold_bars {
        let y = rect.center().y;
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            egui::Stroke::new(1.0, muted),
        );
        painter.text(
            Pos2::new(rect.min.x + 2.0, y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.0),
            muted,
        );
    }

    // Module faceplates: border + title in the top screw zone.
    let outline = rgb(crate::theme::active().physical_skeleton_module_outline);
    let title_color = rgb(crate::theme::active().text);
    for module in &spec.modules {
        painter.rect(
            module.rect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, outline),
            egui::StrokeKind::Inside,
        );
        if !module.title.is_empty() && module.rect.height() >= 12.0 {
            painter.text(
                module.rect.min + egui::vec2(4.0, 2.0),
                egui::Align2::LEFT_TOP,
                &module.title,
                egui::FontId::proportional(11.0),
                dim(title_color, spec.paused),
            );
        }
    }

    // Element cells: skeleton markers, or the full state glyph + label +
    // state text (fader face for fader modules).
    for cell in &spec.cells {
        paint_cell(painter, cell, spec.skeleton, spec.paused);
    }

    ctx.input(|i| physical_frame(i, spec.cell_scale))
}

/// Draw one element cell: skeleton marker, compact state cell, or the fader
/// face (a bottom-up vertical LED strip with the label and percentage right
/// of the track), mirroring `ui.rs::render_physical_cell` /
/// `render_fader_track` / the skeleton cell rendering. Shared with the
/// panels pane (`panels::paint_panels`), which draws the same cells inside
/// its bordered container.
pub(super) fn paint_cell(painter: &Painter, cell: &CellSpec, skeleton: bool, paused: bool) {
    let rect = cell.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    if skeleton {
        let (marker, color) = match cell.mark {
            PortMark::PortIn => ("\u{25C0}", crate::theme::active().cv_in),
            PortMark::PortOut => ("\u{25B6}", crate::theme::active().cv_out),
            PortMark::Cell => ("\u{00B7}", crate::theme::active().muted),
        };
        let size = (rect.height() * 0.6).clamp(6.0, 14.0);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            marker,
            egui::FontId::monospace(size),
            dim(rgb(color), paused),
        );
        return;
    }

    // Hover / selected-circuit hardware emphasis: a muted backdrop.
    if cell.highlighted {
        let mut bg = rgb(crate::theme::active().muted);
        bg = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 90);
        painter.rect_filled(rect, 0.0, bg);
    }

    let base = cell.shift_color.unwrap_or(cell.color);

    if cell.is_fader {
        paint_fader(painter, cell, base, paused);
        return;
    }

    // Compact cell: state glyph always; the label joins the first row when
    // the cell is wide enough; the state text takes the second row.
    let font = egui::FontId::monospace((rect.height() * 0.42).clamp(6.0, 15.0));
    let line_h = font.size * 1.25;
    let color = dim(rgb(base), paused);
    let mut first = cell.glyph.clone();
    if rect.width() >= 28.0 && !cell.label.is_empty() {
        let budget = (rect.width() / font.size).floor() as usize;
        let room = budget.saturating_sub(cell.glyph.chars().count() + 1);
        if room > 0 {
            first.push(' ');
            first.push_str(&clip_label(&cell.label, room));
        }
    }
    painter.text(
        rect.min + egui::vec2(2.0, 0.0),
        egui::Align2::LEFT_TOP,
        &first,
        font.clone(),
        color,
    );
    let state_font = egui::FontId::proportional((font.size * 0.8).max(9.0));
    // The state line renders in the smaller proportional font, so the gate
    // is the glyph line plus that line's height, not two glyph lines (which
    // rejects cells that do fit both lines).
    if rect.height() >= line_h + state_font.size * 1.25 {
        let budget = (rect.width() / state_font.size).floor() as usize;
        let state = clip_label(&cell.state_text, budget.max(1));
        painter.text(
            rect.min + egui::vec2(2.0, line_h),
            egui::Align2::LEFT_TOP,
            state,
            state_font,
            dim(rgb(crate::theme::active().muted), paused),
        );
    }
}

/// The fader face: a bottom-up vertical LED strip filling `value × height`,
/// lit in the amber `fader_led_bar` token with the unfilled part in muted
/// shade, and the label + percentage sharing the columns right of the track
/// when the cell is wide enough (port of `ui.rs::render_fader_track`).
fn paint_fader(painter: &Painter, cell: &CellSpec, base: Color, paused: bool) {
    let rect = cell.rect;
    let track_w = rect.width().min(10.0).max(2.0);
    let value = cell.fader_value.clamp(0.0, 1.0);
    let fill_h = (rect.height() * value).round().clamp(0.0, rect.height());
    let track = Rect::from_min_size(rect.min, egui::vec2(track_w, rect.height()));
    if fill_h > 0.0 {
        let lit = Rect::from_min_size(
            Pos2::new(track.min.x, track.max.y - fill_h),
            egui::vec2(track_w, fill_h),
        );
        painter.rect_filled(
            lit,
            0.0,
            dim(rgb(crate::theme::active().fader_led_bar), paused),
        );
    }
    if rect.height() > fill_h {
        let empty = Rect::from_min_size(track.min, egui::vec2(track_w, rect.height() - fill_h));
        painter.rect_filled(empty, 0.0, dim(rgb(crate::theme::active().muted), paused));
    }

    let text_x = rect.min.x + track_w + 3.0;
    let text_w = rect.width() - track_w - 3.0;
    if text_w < 6.0 {
        return;
    }
    let font = egui::FontId::proportional((rect.height() * 0.34).clamp(8.0, 13.0));
    let line_h = font.size * 1.25;
    let color = dim(rgb(base), paused);
    let budget = (text_w / font.size).floor() as usize;
    if budget > 0 && !cell.label.is_empty() {
        painter.text(
            Pos2::new(text_x, rect.min.y),
            egui::Align2::LEFT_TOP,
            clip_label(&cell.label, budget),
            font.clone(),
            color,
        );
    }
    let state_font = egui::FontId::proportional((font.size * 0.8).max(9.0));
    if rect.height() >= line_h + state_font.size * 1.25 {
        let pct = clip_label(&cell.state_text, budget.max(1));
        painter.text(
            Pos2::new(text_x, rect.min.y + line_h),
            egui::Align2::LEFT_TOP,
            pct,
            state_font,
            dim(rgb(crate::theme::active().muted), paused),
        );
    }
}

/// Per-kind component visuals for a cell: state glyph, state text, and base
/// color (port of `ui.rs::physical_visuals`). Buttons pick the shift-group
/// token when the group is active; fader-marked knobs/encoders render the
/// amber LED strip instead of a disc.
pub(crate) fn cell_visuals(
    comp: &crate::patch::HwComponent,
    is_shift_active: bool,
    fader: bool,
) -> (String, String, Color) {
    let t = crate::theme::active();
    match comp.kind {
        ComponentKind::Button => {
            let on = matches!(comp.state, ComponentState::On);
            (
                if on { "\u{25CF}" } else { "\u{25CB}" }.into(),
                if on { "ON" } else { "OFF" }.into(),
                if is_shift_active {
                    match comp.shift_group {
                        Some(ShiftGroup::Group1) => t.shift1,
                        Some(ShiftGroup::Group2) => t.shift2,
                        Some(ShiftGroup::Group3) => t.shift3,
                        Some(ShiftGroup::Group4) => t.shift4,
                        None => t.button,
                    }
                } else {
                    t.button
                },
            )
        }
        ComponentKind::CvIn => ("\u{25C0}".into(), "CV IN".into(), t.cv_in),
        ComponentKind::CvOut => ("\u{25B6}".into(), "CV OUT".into(), t.cv_out),
        ComponentKind::Knob | ComponentKind::Encoder if fader => {
            let val = match &comp.state {
                ComponentState::Value(v) => format!("{:.0}%", v * 100.0),
                _ => "---".into(),
            };
            ("\u{25AE}".into(), val, t.fader_led_bar)
        }
        ComponentKind::Knob | ComponentKind::Encoder => {
            let val = match &comp.state {
                ComponentState::Value(v) => format!("{:.0}%", v * 100.0),
                _ => "---".into(),
            };
            ("\u{25C9}".into(), val, t.knob)
        }
        ComponentKind::Switch => {
            let (glyph, state) = match &comp.state {
                ComponentState::Value(v) => ("\u{25C9}", format!("{:.0}%", v * 100.0)),
                ComponentState::On => ("\u{25A3}", "ON".into()),
                _ => ("\u{25A1}", "OFF".into()),
            };
            (glyph.into(), state, t.switch)
        }
        ComponentKind::Led => {
            let on = matches!(comp.state, ComponentState::On);
            (
                if on { "\u{25CF}" } else { "\u{25CB}" }.into(),
                if on { "ON" } else { "OFF" }.into(),
                t.led,
            )
        }
    }
}

/// The rack skeleton's screen geometry in egui points (port of
/// `ui.rs::physical_skeleton_geometry`): the case outline, mounts, labeled
/// fold bars, module outlines, and element-cell rects with their port
/// markers. Pure so tests assert the mm→point mapping without a window.
pub(crate) fn rack_geometry(
    rack: &RackLayout,
    chain: &PhysicalLayout,
    mapping: &ScreenMapping,
    cell_scale: f32,
) -> RackGeometry {
    let scale = cell_scale as f64;
    let cell = |mm: crate::physical::RectMm| -> Rect {
        let (x, y, w, h) = mapping.mm_to_screen(mm);
        Rect::from_min_size(
            Pos2::new((x * scale) as f32, (y * scale) as f32),
            Vec2::new((w * scale) as f32, (h * scale) as f32),
        )
    };
    let case_rect = cell(crate::physical::RectMm {
        x_mm: 0.0,
        y_mm: 0.0,
        w_mm: rack.total_width_mm,
        h_mm: rack.total_height_mm,
    });
    let fold_bars = rack
        .fold_bars
        .iter()
        .map(|f| (cell(f.rect_mm), format!(" {}", f.after_row + 2)))
        .collect();
    let mounts = [
        rack.mounts.top,
        rack.mounts.side_left,
        rack.mounts.side_right,
    ]
    .into_iter()
    .flatten()
    .map(cell)
    .collect();

    let mut module_rects = Vec::new();
    let mut cells = Vec::new();
    for row in &rack.rows {
        for placed in &row.modules {
            let abs_y_mm = row.y_mm + placed.rect_mm.y_mm;
            module_rects.push((
                placed.module_index,
                cell(crate::physical::RectMm {
                    x_mm: placed.rect_mm.x_mm,
                    y_mm: abs_y_mm,
                    w_mm: placed.rect_mm.w_mm,
                    h_mm: placed.rect_mm.h_mm,
                }),
            ));
            let module = &chain.modules[placed.module_index];
            for (cell_index, component) in module.components.iter().enumerate() {
                let Some(cell_mm) = chain.cell_for(placed.module_index, &component.id) else {
                    continue;
                };
                if cell_mm.element.is_none() {
                    continue; // not a faceplate element cell (e.g. fallback)
                }
                let mark = match cell_mm.kind_letter {
                    Some('I') => PortMark::PortIn,
                    Some('O') => PortMark::PortOut,
                    _ => PortMark::Cell,
                };
                let rect = cell(crate::physical::RectMm {
                    x_mm: placed.rect_mm.x_mm + cell_mm.rect_mm.x_mm,
                    y_mm: abs_y_mm + cell_mm.rect_mm.y_mm,
                    w_mm: cell_mm.rect_mm.w_mm,
                    h_mm: cell_mm.rect_mm.h_mm,
                });
                cells.push((placed.module_index, cell_index, rect, mark));
            }
        }
    }

    RackGeometry {
        case_rect,
        mounts,
        fold_bars,
        module_rects,
        cells,
    }
}

/// Pure rack skeleton geometry in points (see [`rack_geometry`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RackGeometry {
    pub case_rect: Rect,
    pub mounts: Vec<Rect>,
    pub fold_bars: Vec<(Rect, String)>,
    pub module_rects: Vec<(usize, Rect)>,
    /// `(module index, cell index, rect, port mark)` in chain order.
    pub cells: Vec<(usize, usize, Rect, PortMark)>,
}

/// Vertical + horizontal mm grid lines mapped to points, one line every
/// `step_mm` across the rack bounds (`w_mm` × `h_mm` at the origin).
pub(crate) fn mm_grid_lines(
    mapping: &ScreenMapping,
    cell_scale: f32,
    w_mm: f64,
    h_mm: f64,
    step_mm: f64,
) -> Vec<(Pos2, Pos2)> {
    let scale = cell_scale as f64;
    let mut lines = Vec::new();
    if step_mm <= 0.0 {
        return lines;
    }
    let mut x = step_mm;
    while x < w_mm {
        let (px, py, _, ph) = mapping.mm_to_screen(crate::physical::RectMm {
            x_mm: x,
            y_mm: 0.0,
            w_mm: 0.0,
            h_mm,
        });
        lines.push((
            Pos2::new((px * scale) as f32, (py * scale) as f32),
            Pos2::new((px * scale) as f32, ((py + ph) * scale) as f32),
        ));
        x += step_mm;
    }
    let mut y = step_mm;
    while y < h_mm {
        let (px, py, pw, _) = mapping.mm_to_screen(crate::physical::RectMm {
            x_mm: 0.0,
            y_mm: y,
            w_mm: w_mm,
            h_mm: 0.0,
        });
        lines.push((
            Pos2::new((px * scale) as f32, (py * scale) as f32),
            Pos2::new(((px + pw) * scale) as f32, (py * scale) as f32),
        ));
        y += step_mm;
    }
    lines
}

/// The frame's physical-view input from the egui input state: middle-drag
/// pan (points → screen cells via `cell_scale`), wheel zoom about the cursor
/// (same sensitivity/clamp as the graph canvas), and a plain `s` skeleton
/// toggle. Pure so input tests run without a window.
pub(super) fn physical_frame(i: &egui::InputState, cell_scale: f32) -> PhysicalFrame {
    let mut frame = PhysicalFrame::default();
    if i.pointer.middle_down() {
        let d = i.pointer.delta();
        let scale = cell_scale.max(f32::EPSILON);
        frame.pan_delta = (d.x / scale, d.y / scale);
    }
    let pointer = i.pointer.latest_pos();
    if i.smooth_scroll_delta.y.abs() > f32::EPSILON {
        let factor = ((-i.smooth_scroll_delta.y) * ZOOM_SENSITIVITY).exp();
        let factor = factor.clamp(1.0 / MAX_ZOOM_STEP, MAX_ZOOM_STEP);
        if let Some(p) = pointer {
            frame.zoom = Some((factor, (p.x, p.y)));
        }
    }
    if i.key_pressed(egui::Key::S)
        && !(i.modifiers.shift || i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command)
    {
        frame.skeleton_toggle = true;
    }
    frame
}

fn rgb(color: Color) -> egui::Color32 {
    crate::theme::active().egui_color(color)
}

/// Dim a color when processing is paused (the terminal `DIM` modifier).
fn dim(color: egui::Color32, paused: bool) -> egui::Color32 {
    if paused {
        color.gamma_multiply(0.55)
    } else {
        color
    }
}

/// Truncate a label to `max_chars` characters with an ellipsis when it
/// overflows (port of `ui.rs::truncate_with_ellipsis`).
fn clip_label(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{ComponentState, HwComponent};
    use crate::physical::{ChainGaps, PlacedModule, RackMounts, RackRowPlacement};

    /// One faceplate with a button + a knob, packed into a single row.
    fn fixture() -> (RackLayout, PhysicalLayout) {
        let chain = PhysicalLayout {
            modules: vec![crate::physical::PhysicalModule {
                controller: "P2B8".into(),
                module_instance: Some(1),
                geometry_key: "p2b8".into(),
                is_fallback: false,
                rect_mm: crate::physical::RectMm {
                    x_mm: 0.0,
                    y_mm: 0.0,
                    w_mm: 200.0,
                    h_mm: 40.0,
                },
                width_hp: 12.0,
                he: 1,
                cells: {
                    let mut map = std::collections::HashMap::new();
                    map.insert(
                        "B".into(),
                        vec![crate::physical::ElementCell {
                            family: "B".into(),
                            col: 0,
                            row: 0,
                            rect_mm: crate::physical::RectMm {
                                x_mm: 10.0,
                                y_mm: 5.0,
                                w_mm: 20.0,
                                h_mm: 10.0,
                            },
                            label: "B1.1".into(),
                            element: Some(1),
                            kind_letter: Some('B'),
                        }],
                    );
                    map
                },
                components: vec![
                    HwComponent {
                        id: "B1.1".into(),
                        label: "B1.1".into(),
                        kind: ComponentKind::Button,
                        shift_group: None,
                        state: ComponentState::On,
                        controller: "P2B8".into(),
                        led: None,
                    },
                    HwComponent {
                        id: "P1.1".into(),
                        label: "P1.1".into(),
                        kind: ComponentKind::Knob,
                        shift_group: None,
                        state: ComponentState::Value(0.5),
                        controller: "P2B8".into(),
                        led: None,
                    },
                ],
            }],
            total_width_mm: 200.0,
            total_height_mm: 40.0,
            chain_gaps_mm: ChainGaps::default(),
            hp_mm: 16.6667,
            he_mm: std::collections::HashMap::new(),
            fallback_width_mm: 0.0,
            fallback_height_mm: 0.0,
        };
        let rack = RackLayout {
            rows: vec![RackRowPlacement {
                he: 1,
                hp: 12.0,
                label: Some("row 1".into()),
                y_mm: 0.0,
                height_mm: 40.0,
                modules: vec![PlacedModule {
                    key: "P2B8 1".into(),
                    module_index: 0,
                    rect_mm: crate::physical::RectMm {
                        x_mm: 0.0,
                        y_mm: 0.0,
                        w_mm: 200.0,
                        h_mm: 40.0,
                    },
                    overridden: false,
                }],
                fill_width_mm: 200.0,
            }],
            fold_bars: vec![],
            mounts: RackMounts::default(),
            total_width_mm: 200.0,
            total_height_mm: 40.0,
            fold_bar_height_mm: 6.0,
        };
        (rack, chain)
    }

    fn mapping() -> ScreenMapping {
        ScreenMapping::new(
            crate::physical::PHYSICAL_COLS_PER_MM,
            crate::physical::PHYSICAL_ROWS_PER_MM,
            1.0,
            0.0,
            0.0,
        )
    }

    /// The resolved spec the paint tests draw: geometry via `rack_geometry`,
    /// cell visuals via `cell_visuals`, one cell per fixture component.
    fn spec(skeleton: bool, paused: bool) -> PhysicalSpec {
        let (rack, chain) = fixture();
        let m = mapping();
        let geom = rack_geometry(&rack, &chain, &m, 10.0);
        let mut cells = Vec::new();
        for &(mi, ci, rect, mark) in &geom.cells {
            let comp = &chain.modules[mi].components[ci];
            let (glyph, state_text, color) = cell_visuals(comp, false, false);
            cells.push(CellSpec {
                rect,
                glyph,
                label: comp.label.clone(),
                state_text,
                color,
                is_fader: false,
                fader_value: 0.0,
                global_index: ci,
                mark,
                highlighted: false,
                shift_color: None,
                kind: comp.kind.clone(),
            });
        }
        let modules = geom
            .module_rects
            .iter()
            .map(|&(mi, rect)| ModuleSpec {
                rect,
                title: format!("{} {}", chain.modules[mi].controller, 1),
            })
            .collect();
        PhysicalSpec {
            background: crate::theme::active().graph_canvas_bg,
            case_rect: geom.case_rect,
            mounts: geom.mounts,
            fold_bars: geom.fold_bars,
            modules,
            cells,
            grid_lines: mm_grid_lines(&m, 10.0, 200.0, 40.0, 20.0),
            cell_scale: 10.0,
            skeleton,
            paused,
        }
    }

    #[test]
    fn rack_geometry_maps_mm_to_points() {
        let (rack, chain) = fixture();
        let geom = rack_geometry(&rack, &chain, &mapping(), 10.0);
        // 200×40 mm at 0.15 cols/mm × 10 px = 300×120 points.
        assert_eq!(
            geom.case_rect,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 120.0))
        );
        assert_eq!(geom.module_rects.len(), 1);
        assert_eq!(geom.module_rects[0].0, 0);
        assert_eq!(
            geom.module_rects[0].1,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 120.0))
        );
        // The B1.1 cell: 20×10 mm at (10,5) mm → 3×3 cells at (1.5,1.5) = 30×30 px at (15,15).
        assert_eq!(geom.cells.len(), 1);
        let (_, _, rect, mark) = geom.cells[0];
        assert_eq!(mark, PortMark::Cell);
        assert!((rect.min.x - 15.0).abs() < 1e-3);
        assert!((rect.min.y - 15.0).abs() < 1e-3);
        assert!((rect.width() - 30.0).abs() < 1e-3);
        assert!((rect.height() - 30.0).abs() < 1e-3);
    }

    #[test]
    fn mm_grid_lines_span_the_rack_at_step_intervals() {
        let lines = mm_grid_lines(&mapping(), 10.0, 200.0, 40.0, 20.0);
        // 20mm = 3 cells = 30 px; verticals at 30..270, horizontals at 30.
        let verts = lines.iter().filter(|(a, b)| a.x == b.x).count();
        let hors = lines.iter().filter(|(a, b)| a.y == b.y).count();
        assert_eq!(verts, 9);
        assert_eq!(hors, 1);
    }

    #[test]
    fn cell_visuals_port_fidelity() {
        let t = crate::theme::active();
        let on = HwComponent {
            id: "B1.1".into(),
            label: "B1.1".into(),
            kind: ComponentKind::Button,
            shift_group: None,
            state: ComponentState::On,
            controller: "P2B8".into(),
            led: None,
        };
        let (g, s, c) = cell_visuals(&on, false, false);
        assert_eq!(g, "\u{25CF}");
        assert_eq!(s, "ON");
        assert_eq!(c, t.button);
        // Shift-active button takes the group token.
        let shifted = HwComponent {
            shift_group: Some(ShiftGroup::Group2),
            ..on.clone()
        };
        let (_, _, c) = cell_visuals(&shifted, true, false);
        assert_eq!(c, t.shift2);
        // Fader-marked knob renders the LED strip state.
        let knob = HwComponent {
            kind: ComponentKind::Knob,
            state: ComponentState::Value(0.5),
            ..on
        };
        let (g, s, c) = cell_visuals(&knob, false, true);
        assert_eq!(g, "\u{25AE}");
        assert_eq!(s, "50%");
        assert_eq!(c, t.fader_led_bar);
    }

    fn painted_labels(spec: &PhysicalSpec) -> Vec<String> {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_physical(ui.painter(), ui.max_rect().size(), ui.ctx(), Some(spec));
        });
        let labels: Vec<String> = full_output
            .shapes
            .iter()
            .filter_map(|cs| {
                if let egui::epaint::Shape::Text(t) = &cs.shape {
                    Some(t.galley.text().to_string())
                } else {
                    None
                }
            })
            .collect();
        // The real loop applies `textures_delta` to the wgpu texture manager
        // (mod.rs); a headless test drops the output, so release it first
        // (epaint panics on dropping unapplied deltas).
        full_output.textures_delta.clear();
        labels
    }

    #[test]
    fn paint_draws_module_title_and_cell_labels() {
        let labels = painted_labels(&spec(false, false));
        assert!(
            labels.iter().any(|l| l.contains("P2B8 1")),
            "module title drawn: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("\u{25CF}")),
            "button glyph drawn: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("ON")),
            "state text drawn: {labels:?}"
        );
    }

    #[test]
    fn paint_skeleton_draws_markers_and_no_state() {
        let labels = painted_labels(&spec(true, false));
        assert!(
            labels.iter().any(|l| l.contains("\u{00B7}")),
            "cell marker drawn: {labels:?}"
        );
        assert!(
            !labels.iter().any(|l| l.contains("ON")),
            "no state in skeleton: {labels:?}"
        );
    }

    #[test]
    fn paint_none_spec_draws_nothing_and_reports_default_frame() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let mut frame = PhysicalFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = paint_physical(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
        });
        full_output.textures_delta.clear();
        assert_eq!(frame, PhysicalFrame::default());
        assert!(full_output.shapes.is_empty());
    }

    #[test]
    fn physical_frame_reports_pan_zoom_and_skeleton_toggle() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events: vec![
                // Middle button down, then drag 20px right / 10px down.
                egui::Event::PointerButton {
                    pos: Pos2::new(100.0, 100.0),
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerMoved(Pos2::new(120.0, 110.0)),
                // Wheel up (zoom in): positive Y scrolls down in the shared
                // graph convention, so wheel-up is a negative delta.
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -50.0),
                    modifiers: egui::Modifiers::default(),
                    phase: egui::TouchPhase::Move,
                },
                // Plain `s`.
                egui::Event::Key {
                    key: egui::Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            ..Default::default()
        };
        // Frame 1 parks the pointer at (100,100) so frame 2's move has a
        // previous position to measure the middle-drag delta against.
        let park = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events: vec![egui::Event::PointerMoved(Pos2::new(100.0, 100.0))],
            ..Default::default()
        };
        let mut frame = PhysicalFrame::default();
        let mut full_output = ctx.run_ui(park, |_| {});
        full_output.textures_delta.clear();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = ui.ctx().input(|i| physical_frame(i, 10.0));
        });
        full_output.textures_delta.clear();
        // 20px drag at 10px/cell → 2 cells right, 1 down.
        assert_eq!(frame.pan_delta, (2.0, 1.0));
        // Wheel up 50px → zoom-in factor > 1 anchored at the cursor.
        let (factor, anchor) = frame.zoom.expect("wheel zoom reported");
        assert!(factor > 1.0, "wheel-up zooms in: {factor}");
        assert_eq!(anchor, (120.0, 110.0));
        assert!(frame.skeleton_toggle, "plain `s` toggles the skeleton");
    }
}
