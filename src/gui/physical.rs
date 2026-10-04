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
//! Tests build [`PhysicalSpec`] directly via the shared [`rack_geometry`],
//! the [`ScreenMapping`] (mm↔screen cells, also pure), and the display
//! labels from `Patch::display_label` semantics.

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::app::App;
use crate::patch::{ComponentKind, ComponentState, NodeId, ShiftGroup};
use crate::physical::{PhysicalLayout, RackLayout, RackSpec, ScreenMapping};
use crate::theme::Color;

use super::graph::{MAX_ZOOM_STEP, ZOOM_SENSITIVITY};

/// Fixed screen-space font (points) for a module-UI cell label: the label
/// keeps this size at every `physical_zoom`, so zooming the rack never grows
/// or shrinks label text (label-guess-and-screen-scale, design decision 4).
/// 10 pt sits mid-range in the old height-derived `height × 0.42` clamp
/// (6–15 pt), matching a typical ~24 pt cell height.
const CELL_LABEL_SIZE: f32 = 10.0;
/// Monospace advance as a fraction of the font size (egui's monospace face is
/// close to 0.6 em — mirrors `gui::graph`'s `MONO_ADVANCE`), used to budget
/// the label characters that fit a cell at the fixed [`CELL_LABEL_SIZE`] font.
const MONO_ADVANCE: f32 = 0.6;

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
/// (hover/circuit highlight, shift color, paused dim). `component_index` is the
/// component's index into `patch.hw_components` (`None` for unused geometry
/// cells), so the window can hit-test the module UI against `component_rects`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellSpec {
    pub rect: Rect,
    pub glyph: String,
    pub label: String,
    pub state_text: String,
    pub color: Color,
    pub is_fader: bool,
    pub fader_value: f32,
    pub component_index: Option<usize>,
    pub mark: PortMark,
    pub highlighted: bool,
    pub shift_color: Option<Color>,
    /// When set, paint a transparent wash backdrop in this cell's color to visualize
    /// the modifier hold state (mouse Down without keyboard modifiers on a
    /// modifier-eligible component).
    pub modifier_wash: Option<Color>,
    /// When any modifier is active and this cell is NOT in its influence set,
    /// dim the cell content (design D: unaffected cells dim while a modifier
    /// is held or latched). Orthogonal to the paused dim.
    pub dimmed: bool,
    /// Component kind: lets the input layer tell knobs/encoders apart for
    /// wheel-over-component semantics.
    pub kind: crate::patch::ComponentKind,
    /// LED state folded into this element (if any)
    pub led_state: Option<f32>,
    /// LED RGB colour folded into this element (if any)
    pub led_rgb: Option<[u8; 3]>,
}

/// Fixed screen-space font (points) for a performance-view callout headline:
/// big next to the 10 pt in-cell labels, so the exploded field reads at a
/// glance (performance-view 1.3). Painted monospace so the measured callout
/// size (via [`MONO_ADVANCE`]) matches what egui draws.
const CALLOUT_LABEL_SIZE: f32 = 14.0;
/// Padding inside a callout rect, in points.
const CALLOUT_PAD: f32 = 4.0;

/// One exploded element label for painting: the placed rect outside the rack
/// rect, the leader-line anchor on the host cell edge, the resolved headline
/// (store → guess → derived), the live state line, and the element color.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CalloutSpec {
    pub label_rect: Rect,
    pub anchor: Pos2,
    pub label: String,
    pub state_text: String,
    pub color: Color,
    pub highlighted: bool,
}

/// The performance-view overlay payload for one frame: the screen-space rack
/// bounds the placement kept clear of, plus one callout per labelled element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PerformanceOverlay {
    pub rack_rect: Rect,
    pub callouts: Vec<CalloutSpec>,
}

/// A DB8E OLED display band (`db8e-oled-display-placeholder`): the bordered
/// upper-band rect above the B-grid plus the centered state text derived
/// from the patch. Decorative only; it carries no `CellSpec`, so it never
/// publishes a hit rect.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Db8eBand {
    pub rect: Rect,
    pub state: &'static str,
}

/// The fully-resolved physical-view payload for one frame. Pure data:
/// tests build it directly; [`paint_physical`] only draws it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PhysicalSpec {
    pub background: Color,
    pub case_rect: Rect,
    pub mounts: Vec<Rect>,
    pub fold_bars: Vec<(Rect, String)>,
    pub modules: Vec<ModuleSpec>,
    pub cells: Vec<CellSpec>,
    /// DB8E OLED display bands (decorative, no hit rects).
    pub db8e_bands: Vec<Db8eBand>,
    /// Vertical + horizontal mm grid lines, already mapped to points.
    pub grid_lines: Vec<(Pos2, Pos2)>,
    /// Points per screen cell: the egui-layer scale over the shared
    /// `ScreenMapping` (mm→cells), used to convert pointer pan deltas back
    /// into cell units for `App::physical_offset`.
    pub cell_scale: f32,
    /// Performance-view overlay (compact rack + exploded callouts), present
    /// exactly when `App.showing_performance` was on at spec-build time.
    pub performance: Option<PerformanceOverlay>,
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

/// Points per screen cell: the egui-layer scale over the shared
/// `ScreenMapping` (mm→cells). Mirrors the test builder's 10 px/cell so the
/// production pane and the headless paint tests share one geometry.
const PHYSICAL_CELL_SCALE: f32 = 10.0;

/// The app-driven physical spec: build the controller chain from the loaded
/// patch (`PhysicalLayout::build`), pack it into the configured rack (or the
/// default case when none is set), and resolve cells/geometry under the
/// app's physical zoom/offset, anchored at `pane`'s top-left corner. `None`
/// when no patch is loaded — the slot then paints nothing. Mirrors the
/// test-only `build_spec`.
///
/// `pane` is the on-screen rect the rack is drawn into: the module UI can
/// occupy any pane of the arrangement (BigLeft at startup, but BigRight or a
/// small pane after a swap), so the rack-local mm geometry must be translated
/// by the pane origin — otherwise the drawn rack and the published
/// `component_rects` hit geometry stay at the window origin and stop matching
/// the pane (and each other) as soon as the module UI is not in BigLeft.
pub(crate) fn physical_spec(app: &App, pane: egui::Rect) -> Option<PhysicalSpec> {
    let patch = app.patch.as_ref()?;
    let chain = PhysicalLayout::build(patch);
    let rack_spec = if app.physical_rack_spec.rows.is_empty() {
        RackSpec::default_case(&chain)
    } else {
        app.physical_rack_spec.clone()
    };
    let rack = RackLayout::pack(&chain, &rack_spec);
    // `mm_to_screen` returns screen cells and `rack_geometry` scales them to
    // points, so the pane origin is folded into the offset in cell units
    // (offset is subtracted: `screen = mm·factor·zoom − offset`). The
    // performance view ignores the user pan/zoom and centers a compact
    // (~1/4-area) rack instead; the overlay keeps label rects clear of it.
    let pane_dx = pane.min.x as f64 / PHYSICAL_CELL_SCALE as f64;
    let pane_dy = pane.min.y as f64 / PHYSICAL_CELL_SCALE as f64;
    let normal_mapping = || {
        ScreenMapping::new(
            crate::physical::PHYSICAL_COLS_PER_MM,
            crate::physical::PHYSICAL_ROWS_PER_MM,
            app.physical_zoom as f64,
            app.physical_offset.0 as f64 - pane_dx,
            app.physical_offset.1 as f64 - pane_dy,
        )
    };
    let m = if app.showing_performance {
        performance_mapping(&rack, pane).unwrap_or_else(normal_mapping)
    } else {
        normal_mapping()
    };
    let geom = rack_geometry(&rack, &chain, &m, PHYSICAL_CELL_SCALE);
    let mut cells = Vec::new();

    // The physical chain clones components per module, so the module-local cell
    // index is not the `patch.hw_components` index the handler's
    // `component_rects` hit-testing uses. Map by token id to recover the global
    // index for every real element.
    let index_of: std::collections::HashMap<&str, usize> = patch
        .hw_components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    // Persistent selection emphasis (droid_tui-8ia): the shared element
    // selection (`selected_component`) keeps the clicked cell marked after the
    // pointer leaves, and the graph-node selection (`selected_circuit`) marks
    // every hardware cell of that circuit's section. Both reuse the hover
    // backdrop below so the module UI shows one selection language.
    let selected_circuit_cells: std::collections::HashSet<usize> =
        app.circuit_hw_token_indices().into_iter().collect();

    // Track which LED tokens are folded into elements (so we don't render them standalone)
    let mut folded_leds: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Build a mapping from element token to its LED tokens for each module
    let mut element_leds: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for module in chain.modules.iter() {
        for comp in &module.components {
            if let Some(led_token) = &comp.led {
                element_leds
                    .entry(comp.id.clone())
                    .or_default()
                    .push(led_token.clone());
                folded_leds.insert(led_token.clone());
            }
            // Also add positional LEDs from the resolver
            let positional =
                crate::patch::positional_leds_for_element(&module.controller, &comp.id);
            for led in positional {
                element_leds
                    .entry(comp.id.clone())
                    .or_default()
                    .push(led.clone());
                folded_leds.insert(led);
            }
        }
    }

    // Render all element cells from geometry (including unused ones)
    for &(mi, ci, rect, mark) in &geom.cells {
        let module = &chain.modules[mi];

        // Global `patch.hw_components` index for a real element; `None` for an
        // unused geometry cell (rendered dimmed and unlabelled, never hit).
        let component_index = if ci < module.components.len() {
            index_of.get(module.components[ci].id.as_str()).copied()
        } else {
            None
        };

        // ci might be out of bounds for patch components if it's an unused element
        // In that case, we still render the cell but dimmed and unlabelled
        let (glyph, state_text, color, label, kind, is_fader, fader_value, led_state, led_rgb) =
            if ci < module.components.len() {
                let comp = &module.components[ci];
                let (glyph, state_text, color) = cell_visuals(comp, false, false);
                // Check if this component has associated LEDs
                let leds = element_leds.get(&comp.id).cloned().unwrap_or_default();
                let led_state = leds.first().and_then(|led| {
                    module
                        .components
                        .iter()
                        .find(|c| c.id == *led)
                        .and_then(|c| match &c.state {
                            crate::patch::ComponentState::Value(v) => Some(*v),
                            crate::patch::ComponentState::On => Some(1.0),
                            crate::patch::ComponentState::Off => Some(0.0),
                            _ => None,
                        })
                });
                let led_rgb = leds.first().and_then(|led| {
                    module
                        .components
                        .iter()
                        .find(|c| c.id == *led)
                        .and_then(|c| match &c.state {
                            crate::patch::ComponentState::Led { value: _, rgb } => *rgb,
                            _ => None,
                        })
                });
                // Use shift-aware label resolver
                let shift = app.active_shift.map_or(1, |g| g as u8);
                let layers_enabled = app.labels.layers_enabled;
                let max_shift_layer = app.labels.max_shift_layer;
                let hw_store = app.current_hw_store();
                let label = patch.display_label(
                    &comp.id,
                    shift,
                    layers_enabled,
                    max_shift_layer,
                    &hw_store,
                );
                (
                    glyph, state_text, color, label, comp.kind, false, 0.0, led_state, led_rgb,
                )
            } else {
                // Unused element - render dimmed and unlabelled
                let family = match mark {
                    crate::gui::physical::PortMark::PortIn => "CV",
                    crate::gui::physical::PortMark::PortOut => "CV",
                    _ => "B", // default
                };
                let (glyph, state_text, color) = match family {
                    "CV" => ("\u{25C0}".into(), "".into(), crate::theme::active().cv_in),
                    _ => ("\u{00B7}".into(), "".into(), crate::theme::active().muted),
                };
                (
                    glyph,
                    state_text,
                    color,
                    "".into(),
                    crate::patch::ComponentKind::Button,
                    false,
                    0.0,
                    None,
                    None,
                )
            };

        let is_dimmed = label.is_empty(); // unused elements are dimmed
        cells.push(CellSpec {
            rect,
            glyph,
            label,
            state_text,
            color,
            is_fader,
            fader_value,
            component_index,
            mark,
            highlighted: component_index.is_some_and(|i| {
                app.hovered_component == Some(i)
                    || selected_circuit_cells.contains(&i)
                    || app.selected_component.as_deref() == Some(module.components[ci].id.as_str())
            }),
            shift_color: None,
            modifier_wash: None,
            dimmed: is_dimmed,
            kind,
            led_state,
            led_rgb,
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
    let db8e_bands = geom
        .db8e_bands
        .iter()
        .map(|&(_, rect)| Db8eBand {
            rect,
            state: crate::physical::db8e_display_state_for_layout(&chain),
        })
        .collect();
    let performance = if app.showing_performance {
        Some(performance_overlay(patch, app, geom.case_rect, &cells))
    } else {
        None
    };
    Some(PhysicalSpec {
        performance,
        background: crate::theme::active().graph_canvas_bg,
        case_rect: geom.case_rect,
        mounts: geom.mounts,
        fold_bars: geom.fold_bars,
        modules,
        cells,
        db8e_bands,
        grid_lines: mm_grid_lines(
            &m,
            PHYSICAL_CELL_SCALE,
            rack.total_width_mm,
            rack.total_height_mm,
            20.0,
        ),
        cell_scale: PHYSICAL_CELL_SCALE,
        skeleton: app.physical_show_skeleton,
        paused: app.processing_paused,
    })
}

/// Centered compact mapping for the performance view (performance-view 1.3):
/// the rack renders at half the fit zoom — half-linear in each dimension, so
/// ~1/4 of the pane area — centered on the pane, ignoring the user pan/zoom.
/// `None` on degenerate input (empty pane or rack); the caller falls back to
/// the normal mapping. Pure math, no App dependency, so tests drive it headless.
pub(crate) fn performance_mapping(rack: &RackLayout, pane: egui::Rect) -> Option<ScreenMapping> {
    let cols = crate::physical::PHYSICAL_COLS_PER_MM;
    let rows = crate::physical::PHYSICAL_ROWS_PER_MM;
    let pane_w = pane.width() as f64 / PHYSICAL_CELL_SCALE as f64;
    let pane_h = pane.height() as f64 / PHYSICAL_CELL_SCALE as f64;
    if pane_w <= 0.0 || pane_h <= 0.0 {
        return None;
    }
    if rack.total_width_mm <= 0.0 || rack.total_height_mm <= 0.0 {
        return None;
    }
    let fit = (pane_w / (rack.total_width_mm * cols)).min(pane_h / (rack.total_height_mm * rows));
    if !fit.is_finite() || fit <= 0.0 {
        return None;
    }
    let zoom = fit * 0.5;
    // Pane-relative cells center on the pane middle; the pane origin folds in
    // exactly like the normal mapping so points land window-absolute.
    let pane_dx = pane.min.x as f64 / PHYSICAL_CELL_SCALE as f64;
    let pane_dy = pane.min.y as f64 / PHYSICAL_CELL_SCALE as f64;
    let offset_x = (rack.total_width_mm * 0.5) * cols * zoom - pane_dx - pane_w * 0.5;
    let offset_y = (rack.total_height_mm * 0.5) * rows * zoom - pane_dy - pane_h * 0.5;
    Some(ScreenMapping::new(cols, rows, zoom, offset_x, offset_y))
}

/// Measured callout size for `headline` + `state` at [`CALLOUT_LABEL_SIZE`]:
/// the wider line sets the width, one or two text lines plus padding set the
/// height. Pure so the placement tests budget honest rects.
fn callout_size(headline: &str, state: &str) -> (f32, f32) {
    let chars = headline.chars().count().max(state.chars().count()).max(1);
    let w = chars as f32 * MONO_ADVANCE * CALLOUT_LABEL_SIZE + 2.0 * CALLOUT_PAD;
    let lines = if state.is_empty() { 1 } else { 2 };
    let h = lines as f32 * CALLOUT_LABEL_SIZE * 1.25 + 2.0 * CALLOUT_PAD;
    (w, h)
}

/// The circuit section owning a token occurrence: the last section whose
/// header line precedes the occurrence line (preamble occurrences have no
/// owner). The instance index counts earlier same-name sections, matching
/// the `NodeId::circuit(name, instance)` convention the graph build uses.
pub(crate) fn owning_circuit_node(patch: &crate::patch::Patch, token: &str) -> Option<NodeId> {
    for span in patch.occurrences_for(token) {
        let mut owner: Option<(usize, &str)> = None;
        for (idx, section) in patch.sections.iter().enumerate() {
            if section.header_span.line <= span.line {
                owner = Some((idx, section.name.as_str()));
            } else {
                break;
            }
        }
        let Some((owner_idx, name)) = owner else {
            continue;
        };
        let instance = patch.sections[..owner_idx]
            .iter()
            .filter(|s| s.name == name)
            .count();
        return Some(NodeId::circuit(name, instance));
    }
    None
}

/// Performance callout headline (performance-view 1.3): store-defined →
/// guessed → derived. `fallback` is the `display_label` result, which equals
/// the derived token exactly when no explicit label exists, so it doubles as
/// the derived step without re-resolving the chain.
/// Resolution inputs for [`performance_headline`], bundled so the helper
/// stays under the argument-count lint without an allow attribute.
pub(crate) struct HeadlineCtx<'a> {
    patch: &'a crate::patch::Patch,
    app: &'a App,
    shift: u8,
    layers_enabled: bool,
    max_shift_layer: u8,
    hw_store: &'a std::collections::HashMap<String, std::collections::BTreeMap<u8, String>>,
    circuit_store: &'a std::collections::HashMap<NodeId, String>,
}

pub(crate) fn performance_headline(ctx: &HeadlineCtx, token: &str, fallback: &str) -> String {
    if let Some(explicit) = ctx.patch.explicit_hw_label(
        token,
        ctx.shift,
        ctx.layers_enabled,
        ctx.max_shift_layer,
        ctx.hw_store,
    ) {
        return explicit;
    }
    if let Some(node) = owning_circuit_node(ctx.patch, token) {
        if let Some(stored) = ctx.patch.circuit_label(&node, ctx.circuit_store) {
            return stored;
        }
        if let Some(guess) = ctx.app.guessed_labels.get(&node) {
            return guess.clone();
        }
    }
    fallback.to_string()
}

/// Build the performance overlay from resolved cells: one callout per labelled
/// (real, non-unused) element, placed by `place_labels` clear of the rack
/// rect. Headlines resolve store → guess → derived; state/color/highlight
/// ride the live cell so activation and reset show immediately.
pub(crate) fn performance_overlay(
    patch: &crate::patch::Patch,
    app: &App,
    rack_rect: Rect,
    cells: &[CellSpec],
) -> PerformanceOverlay {
    let shift = app.active_shift.map_or(1, |g| g as u8);
    let layers_enabled = app.labels.layers_enabled;
    let max_shift_layer = app.labels.max_shift_layer;
    let hw_store = app.current_hw_store();
    let circuit_store = app.current_circuit_store();
    let rack = crate::performance::Rect::new(
        rack_rect.min.x,
        rack_rect.min.y,
        rack_rect.width(),
        rack_rect.height(),
    );
    let mut inputs = Vec::new();
    let mut meta: Vec<(String, String, Color, bool)> = Vec::new();
    for cell in cells {
        let Some(hw_idx) = cell.component_index else {
            continue;
        };
        if cell.label.is_empty() {
            continue;
        }
        let Some(comp) = patch.hw_components.get(hw_idx) else {
            continue;
        };
        let ctx = HeadlineCtx {
            patch,
            app,
            shift,
            layers_enabled,
            max_shift_layer,
            hw_store: &hw_store,
            circuit_store: &circuit_store,
        };
        let headline = performance_headline(&ctx, &comp.id, &cell.label);
        let (w, h) = callout_size(&headline, &cell.state_text);
        inputs.push(crate::performance::LabelInput {
            host_id: inputs.len(),
            host_rect: crate::performance::Rect::new(
                cell.rect.min.x,
                cell.rect.min.y,
                cell.rect.width(),
                cell.rect.height(),
            ),
            label_w: w,
            label_h: h,
        });
        meta.push((
            headline,
            cell.state_text.clone(),
            cell.color,
            cell.highlighted,
        ));
    }
    let mut callouts: Vec<CalloutSpec> = crate::performance::place_labels(rack, &inputs)
        .into_iter()
        .map(|placed| {
            let (label, state_text, color, highlighted) = meta[placed.host_id].clone();
            CalloutSpec {
                label_rect: Rect::from_min_size(
                    Pos2::new(placed.label_rect.x, placed.label_rect.y),
                    Vec2::new(placed.label_rect.w, placed.label_rect.h),
                ),
                anchor: Pos2::new(placed.anchor.x, placed.anchor.y),
                label,
                state_text,
                color,
                highlighted,
            }
        })
        .collect();
    // `place_labels` emits host-id order, which is insertion order here, but
    // sort explicitly so the paint order never depends on that guarantee.
    callouts.sort_by_key(|c| (c.label_rect.min.y * 4096.0) as i32);
    PerformanceOverlay {
        rack_rect,
        callouts,
    }
}

/// Paint the physical 1:1 view into `pane`. `None` draws nothing.
/// Returns the frame's pan/zoom/skeleton input report for the loop to apply.
pub(crate) fn paint_physical(
    ui: &mut egui::Ui,
    pane: egui::Rect,
    spec: Option<&PhysicalSpec>,
) -> PhysicalFrame {
    let Some(spec) = spec else {
        return PhysicalFrame::default();
    };
    let painter = ui.painter().with_clip_rect(pane);
    painter.rect_filled(pane, 0.0, rgb(spec.background));

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
        paint_cell(&painter, cell, spec.skeleton, spec.paused);

        // AccessKit annotation for egui_kittest query-by-label
        let response = ui.allocate_rect(cell.rect, egui::Sense::hover());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &cell.label));
    }

    // DB8E OLED display placeholder: bordered upper-band rect with the
    // centered display state (db8e-oled-display-placeholder). Drawn on the
    // shared rack path so skeleton and full presentations coincide;
    // decorative, so it never publishes a hit rect.
    let placeholder = rgb(crate::theme::active().muted);
    for band in &spec.db8e_bands {
        if band.rect.width() < 3.0 || band.rect.height() < 2.0 {
            continue;
        }
        let color = dim(placeholder, spec.paused);
        painter.rect(
            band.rect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            band.rect.center(),
            egui::Align2::CENTER_CENTER,
            band_text(band.state, band.rect.width()),
            egui::FontId::proportional(11.0),
            color,
        );
    }

    // Performance view (performance-view 1.3): exploded callouts over the
    // compact rack — leader line from the host cell edge to the placed label
    // rect, then the big resolved headline plus the live state line.
    // Decorative like the DB8E bands: no hit rects, the cells keep hit-testing.
    if let Some(overlay) = &spec.performance {
        paint_performance(&painter, overlay, spec.paused);
    }

    drop(painter);
    ui.ctx().input(|i| physical_frame(i, spec.cell_scale, pane))
}

/// Draw one element cell: skeleton marker, compact state cell, or the fader
/// face (a bottom-up vertical LED strip with the label and percentage right
/// of the track), mirroring `ui.rs::render_physical_cell` /
/// `render_fader_track` / the skeleton cell rendering. Shared with the
/// module UI's faceplate cells.
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

    // Modifier-hold wash: a low-alpha tinted backdrop on the held component so the
    // user can see which modifier is active while holding the mouse button.
    if let Some(wash) = cell.modifier_wash {
        let mut bg = rgb(wash);
        bg = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 50);
        painter.rect_filled(rect, 0.0, bg);
    }

    let base = cell.shift_color.unwrap_or(cell.color);

    if cell.is_fader {
        paint_fader(painter, cell, base, paused);
        return;
    }

    // Render LED inside the element if present: outline ring plus inset
    // core (fader-led-contrast 2.1). The ring is field chrome and stays
    // full-bright (also while paused); the core carries the value with a
    // brightness floor so an off-LED still shows its field.
    if let Some(led_value) = cell.led_state {
        let level = led_value.clamp(0.0, 1.0).max(0.25);
        let led_color = if let Some(rgb) = cell.led_rgb {
            // Use the RGB colour from the patch, hue preserved
            let scale = |c: u8| ((c as f32 * level).round().clamp(0.0, 255.0)) as u8;
            egui::Color32::from_rgb(scale(rgb[0]), scale(rgb[1]), scale(rgb[2]))
        } else {
            // White-only LED: brightness maps to white intensity
            let intensity = (level * 255.0) as u8;
            egui::Color32::from_rgb(intensity, intensity, intensity)
        };
        // Draw LED indicator in the top-right corner of the cell
        let led_size = (rect.height() * 0.3).clamp(4.0, 10.0);
        let led_rect = Rect::from_min_size(
            Pos2::new(rect.max.x - led_size - 2.0, rect.min.y + 2.0),
            Vec2::new(led_size, led_size),
        );
        painter.rect_stroke(
            led_rect,
            led_size * 0.3,
            egui::Stroke::new(1.0, rgb(crate::theme::active().led_ring)),
            egui::StrokeKind::Inside,
        );
        let core = led_rect.shrink(led_size * 0.25);
        painter.rect_filled(core, led_size * 0.2, led_color);
    }

    // Compact cell: state glyph always; the label shares the first row at the
    // fixed screen-space size once the cell is at least one character of that
    // font wide (label-guess-and-screen-scale, task 2.2); the state text
    // takes the second row.
    let font = egui::FontId::monospace(CELL_LABEL_SIZE);
    let line_h = font.size * 1.25;
    let color = dim(rgb(base), paused || cell.dimmed);
    let mut first = cell.glyph.clone();
    // One character of the fixed font is the hide threshold: a narrower cell
    // renders the glyph but no label text at all.
    let char_w = MONO_ADVANCE * CELL_LABEL_SIZE;
    if rect.width() >= char_w && !cell.label.is_empty() {
        let budget = (rect.width() / char_w).floor() as usize;
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

/// Draw the performance overlay: one leader line plus a boxed big-label
/// callout per placed element. Headlines paint at [`CALLOUT_LABEL_SIZE`]
/// monospace (the size [`callout_size`] budgets); the state line paints
/// smaller and muted like the in-cell state text.
fn paint_performance(painter: &Painter, overlay: &PerformanceOverlay, paused: bool) {
    let leader = rgb(crate::theme::active().performance_leader);
    let headline_font = egui::FontId::monospace(CALLOUT_LABEL_SIZE);
    let state_font = egui::FontId::proportional((CALLOUT_LABEL_SIZE * 0.8).max(9.0));
    for callout in &overlay.callouts {
        let rect = callout.label_rect;
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        // Leader line: host-cell edge anchor to the nearest point of the
        // placed label rect (its edge facing the host cell).
        let edge = Pos2::new(
            callout.anchor.x.clamp(rect.min.x, rect.max.x),
            callout.anchor.y.clamp(rect.min.y, rect.max.y),
        );
        painter.line_segment([callout.anchor, edge], egui::Stroke::new(1.0, leader));
        if callout.highlighted {
            let mut bg = rgb(crate::theme::active().muted);
            bg = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 90);
            painter.rect_filled(rect, 0.0, bg);
        }
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, leader),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.min + egui::vec2(CALLOUT_PAD, CALLOUT_PAD),
            egui::Align2::LEFT_TOP,
            &callout.label,
            headline_font.clone(),
            dim(rgb(callout.color), paused),
        );
        if !callout.state_text.is_empty() {
            painter.text(
                rect.min + egui::vec2(CALLOUT_PAD, CALLOUT_PAD + CALLOUT_LABEL_SIZE * 1.25),
                egui::Align2::LEFT_TOP,
                &callout.state_text,
                state_font.clone(),
                dim(rgb(crate::theme::active().muted), paused),
            );
        }
    }
}

/// The fader face: a bottom-up vertical LED strip filling `value × height`,
/// lit in the amber `fader_led_bar` token with the unfilled part in muted
/// shade, and the label + percentage sharing the columns right of the track
/// when the cell is wide enough (port of `ui.rs::render_fader_track`).
fn paint_fader(painter: &Painter, cell: &CellSpec, base: Color, paused: bool) {
    let rect = cell.rect;
    let track_w = rect.width().clamp(2.0, 10.0);
    let value = cell.fader_value.clamp(0.0, 1.0);
    let fill_h = (rect.height() * value).round().clamp(0.0, rect.height());
    let track = Rect::from_min_size(rect.min, egui::vec2(track_w, rect.height()));
    if fill_h > 0.0 {
        let lit = Rect::from_min_size(
            Pos2::new(track.min.x, track.max.y - fill_h),
            egui::vec2(track_w, fill_h),
        );
        // The lit strip is a state indicator: it stays full-bright while
        // paused (fader-led-contrast 1.1). Only the unlit track dims.
        painter.rect_filled(lit, 0.0, rgb(crate::theme::active().fader_led_bar));
    }
    if rect.height() > fill_h {
        let empty = Rect::from_min_size(track.min, egui::vec2(track_w, rect.height() - fill_h));
        painter.rect_filled(empty, 0.0, dim(rgb(crate::theme::active().muted), paused));
    }
    // Slot outline + value marker (fader-led-contrast 1.2): the outline
    // reads as hardware at value 0, the marker like a physical cap. Both
    // are state chrome and stay full-bright while paused.
    painter.rect_stroke(
        track,
        0.0,
        egui::Stroke::new(1.0, rgb(crate::theme::active().fader_slot)),
        egui::StrokeKind::Inside,
    );
    if fill_h > 0.0 {
        let marker_h = if track_w >= 6.0 { 2.0 } else { 1.0 };
        let marker_y =
            (track.max.y - fill_h - marker_h / 2.0).clamp(track.min.y, track.max.y - marker_h);
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(track.min.x, marker_y),
                egui::vec2(track_w, marker_h),
            ),
            0.0,
            rgb(crate::theme::active().fader_led_bar),
        );
    }

    let text_x = rect.min.x + track_w + 3.0;
    let text_w = rect.width() - track_w - 3.0;
    if text_w < 6.0 {
        return;
    }
    let font = egui::FontId::proportional((rect.height() * 0.34).clamp(8.0, 13.0));
    let line_h = font.size * 1.25;
    let color = dim(rgb(base), paused || cell.dimmed);
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
    let mut db8e_bands = Vec::new();
    for row in &rack.rows {
        for placed in &row.modules {
            let abs_y_mm = row.y_mm + placed.rect_mm.y_mm;
            let module_rect = cell(crate::physical::RectMm {
                x_mm: placed.rect_mm.x_mm,
                y_mm: abs_y_mm,
                w_mm: placed.rect_mm.w_mm,
                h_mm: placed.rect_mm.h_mm,
            });
            module_rects.push((placed.module_index, module_rect));
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

            // DB8E OLED display placeholder (db8e-oled-display-placeholder):
            // the bordered upper-band rect above the B-grid. Band height
            // derives from the module's B-grid top (`cells["B"]` min y_mm,
            // fallback literal 38.0) so geometry drift never hard-codes the
            // constant; the rect insets the faceplate by 1 pt with the module
            // border's two strokes subtracted (port of the terminal
            // placeholder's clipped-rect fix).
            if module.geometry_key == "db8e" {
                let b_top_mm = module
                    .cells
                    .get("B")
                    .and_then(|cells| {
                        cells
                            .iter()
                            .map(|c| c.rect_mm.y_mm)
                            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    })
                    .unwrap_or(38.0);
                let ratio = if module.rect_mm.h_mm > 0.0 {
                    (b_top_mm / module.rect_mm.h_mm).clamp(0.0, 1.0)
                } else {
                    0.295
                };
                let band_h = (module_rect.height() as f64 * ratio).round() as f32;
                if module_rect.width() >= 3.0 && band_h >= 3.0 {
                    db8e_bands.push((
                        placed.module_index,
                        Rect::from_min_size(
                            module_rect.min + Vec2::new(1.0, 1.0),
                            Vec2::new(module_rect.width() - 2.0, band_h - 2.0),
                        ),
                    ));
                }
            }
        }
    }

    RackGeometry {
        case_rect,
        mounts,
        fold_bars,
        module_rects,
        cells,
        db8e_bands,
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
    /// `(module index, band rect)` for DB8E OLED display bands in chain order.
    pub db8e_bands: Vec<(usize, Rect)>,
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
            w_mm,
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
/// toggle. `pane` maps the absolute pointer (wheel zoom anchor) into the
/// pane-local space the spec's cells live in. Pure so input tests run
/// without a window.
pub(super) fn physical_frame(
    i: &egui::InputState,
    cell_scale: f32,
    pane: egui::Rect,
) -> PhysicalFrame {
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
            let local = p - pane.min;
            frame.zoom = Some((factor, (local.x, local.y)));
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

/// Ellipsize the display state to the band width (port of
/// `ui.rs::truncate_with_ellipsis` at draw time): ~6.5 pt per proportional
/// char at the 11 pt band font.
fn band_text(state: &str, width: f32) -> String {
    clip_label(state, (width / 6.5).max(1.0) as usize)
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
                        led_capability: Some(crate::patch::LedCapability::Rgb),
                    },
                    HwComponent {
                        id: "P1.1".into(),
                        label: "P1.1".into(),
                        kind: ComponentKind::Knob,
                        shift_group: None,
                        state: ComponentState::Value(0.5),
                        controller: "P2B8".into(),
                        led: None,
                        led_capability: Some(crate::patch::LedCapability::Rgb),
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

    /// One DB8E faceplate (6 HP, 3 HE; B-grid top at y_mm 38, real geometry
    /// values) packed into a single row, so the band math is production-like.
    fn db8e_fixture() -> (RackLayout, PhysicalLayout) {
        let chain = PhysicalLayout {
            modules: vec![crate::physical::PhysicalModule {
                controller: "DB8E".into(),
                module_instance: Some(1),
                geometry_key: "db8e".into(),
                is_fallback: false,
                rect_mm: crate::physical::RectMm {
                    x_mm: 0.0,
                    y_mm: 0.0,
                    w_mm: 30.48,
                    h_mm: 128.5,
                },
                width_hp: 6.0,
                he: 3,
                cells: {
                    let mut map = std::collections::HashMap::new();
                    map.insert(
                        "B".into(),
                        vec![
                            crate::physical::ElementCell {
                                family: "B".into(),
                                col: 0,
                                row: 0,
                                rect_mm: crate::physical::RectMm {
                                    x_mm: 2.0,
                                    y_mm: 38.0,
                                    w_mm: 8.0,
                                    h_mm: 8.0,
                                },
                                label: "B1.1".into(),
                                element: Some(1),
                                kind_letter: Some('B'),
                            },
                            crate::physical::ElementCell {
                                family: "B".into(),
                                col: 1,
                                row: 0,
                                rect_mm: crate::physical::RectMm {
                                    x_mm: 12.0,
                                    y_mm: 38.0,
                                    w_mm: 8.0,
                                    h_mm: 8.0,
                                },
                                label: "B1.2".into(),
                                element: Some(2),
                                kind_letter: Some('B'),
                            },
                        ],
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
                        controller: "DB8E".into(),
                        led: None,
                        led_capability: Some(crate::patch::LedCapability::Rgb),
                    },
                    HwComponent {
                        id: "B1.2".into(),
                        label: "B1.2".into(),
                        kind: ComponentKind::Button,
                        shift_group: None,
                        state: ComponentState::Off,
                        controller: "DB8E".into(),
                        led: None,
                        led_capability: Some(crate::patch::LedCapability::Rgb),
                    },
                ],
            }],
            total_width_mm: 30.48,
            total_height_mm: 128.5,
            chain_gaps_mm: ChainGaps::default(),
            hp_mm: 16.6667,
            he_mm: std::collections::HashMap::new(),
            fallback_width_mm: 0.0,
            fallback_height_mm: 0.0,
        };
        let rack = RackLayout {
            rows: vec![RackRowPlacement {
                he: 3,
                hp: 6.0,
                label: Some("row 1".into()),
                y_mm: 0.0,
                height_mm: 128.5,
                modules: vec![PlacedModule {
                    key: "DB8E 1".into(),
                    module_index: 0,
                    rect_mm: crate::physical::RectMm {
                        x_mm: 0.0,
                        y_mm: 0.0,
                        w_mm: 30.48,
                        h_mm: 128.5,
                    },
                    overridden: false,
                }],
                fill_width_mm: 30.48,
            }],
            fold_bars: vec![],
            mounts: RackMounts::default(),
            total_width_mm: 30.48,
            total_height_mm: 128.5,
            fold_bar_height_mm: 6.0,
        };
        (rack, chain)
    }

    fn mapping() -> ScreenMapping {
        mapping_at(1.0)
    }

    /// The same mapping at a physical zoom preset — the value
    /// `physical_spec` feeds from `app.physical_zoom`.
    fn mapping_at(zoom: f64) -> ScreenMapping {
        ScreenMapping::new(
            crate::physical::PHYSICAL_COLS_PER_MM,
            crate::physical::PHYSICAL_ROWS_PER_MM,
            zoom,
            0.0,
            0.0,
        )
    }

    /// The resolved spec the paint tests draw: geometry via `rack_geometry`,
    /// cell visuals via `cell_visuals`, one cell per fixture component.
    fn spec(skeleton: bool, paused: bool) -> PhysicalSpec {
        let (rack, chain) = fixture();
        build_spec((&rack, &chain), skeleton, paused, 1.0)
    }

    /// The fixture spec at a physical zoom preset, going through the same
    /// `ScreenMapping` zoom path `physical_spec` takes for `physical_zoom`.
    fn spec_at_zoom(zoom: f64) -> PhysicalSpec {
        let (rack, chain) = fixture();
        build_spec((&rack, &chain), false, false, zoom)
    }

    /// Same builder over a hand-built DB8E rack (one faceplate with a B-grid)
    /// so the band tests exercise the production geometry values.
    fn db8e_spec(skeleton: bool, paused: bool) -> PhysicalSpec {
        let (rack, chain) = db8e_fixture();
        build_spec((&rack, &chain), skeleton, paused, 1.0)
    }

    fn build_spec(
        fx: (&RackLayout, &PhysicalLayout),
        skeleton: bool,
        paused: bool,
        zoom: f64,
    ) -> PhysicalSpec {
        let (rack, chain) = fx;
        let m = mapping_at(zoom);
        let geom = rack_geometry(rack, chain, &m, 10.0);
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
                component_index: Some(ci),
                mark,
                highlighted: false,
                shift_color: None,
                modifier_wash: None,
                dimmed: false,
                kind: comp.kind,
                led_state: None,
                led_rgb: None,
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
        let db8e_bands = geom
            .db8e_bands
            .iter()
            .map(|&(_, rect)| Db8eBand {
                rect,
                state: crate::physical::db8e_display_state_for_layout(chain),
            })
            .collect();
        PhysicalSpec {
            background: crate::theme::active().graph_canvas_bg,
            case_rect: geom.case_rect,
            mounts: geom.mounts,
            fold_bars: geom.fold_bars,
            modules,
            cells,
            db8e_bands,
            grid_lines: mm_grid_lines(&m, 10.0, rack.total_width_mm, rack.total_height_mm, 20.0),
            cell_scale: 10.0,
            performance: None,
            skeleton,
            paused,
        }
    }

    /// A fresh App holding a patch loaded from `path` (the same physical view
    /// defaults `physical_spec` reads: zoom 1, no pan).
    fn app_with_patch(path: &str) -> App {
        let patch = crate::patch::Patch::from_ini_file(std::path::Path::new(path)).unwrap();
        let mut app = App::new();
        assert!(app.load_patch(patch));
        app
    }

    #[test]
    fn physical_spec_component_index_is_the_global_hw_index() {
        // Regression (droid_tui-c4x): the physical chain clones components per
        // module, so a module-local cell index must never be published as the
        // `patch.hw_components` index — otherwise hit-testing a second module's
        // cells resolves to the first module's components.
        let app = app_with_patch("fixtures/multi_module_p2b8.ini");
        let patch = app.patch.as_ref().unwrap();
        let chain = PhysicalLayout::build(patch);
        let rack_spec = if app.physical_rack_spec.rows.is_empty() {
            RackSpec::default_case(&chain)
        } else {
            app.physical_rack_spec.clone()
        };
        let rack = RackLayout::pack(&chain, &rack_spec);
        let m = ScreenMapping::new(
            crate::physical::PHYSICAL_COLS_PER_MM,
            crate::physical::PHYSICAL_ROWS_PER_MM,
            app.physical_zoom as f64,
            app.physical_offset.0 as f64,
            app.physical_offset.1 as f64,
        );
        let geom = rack_geometry(&rack, &chain, &m, PHYSICAL_CELL_SCALE);
        // A zero-origin pane keeps the spec rack-local so it matches `geom`.
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
        assert_eq!(spec.cells.len(), geom.cells.len());
        for (cell, &(mi, ci, rect, _)) in spec.cells.iter().zip(geom.cells.iter()) {
            let comp = &chain.modules[mi].components[ci];
            let expected = patch
                .hw_components
                .iter()
                .position(|c| c.id == comp.id)
                .expect("every chain component exists in the patch");
            assert_eq!(cell.component_index, Some(expected), "cell {}", comp.id);
            assert_eq!(cell.rect, rect);
        }
        // The second module's cells carry global indices past the first module's
        // component count, which the module-local `ci` could not produce.
        let first_module_len = chain.modules[0].components.len();
        assert!(
            spec.cells
                .iter()
                .any(|c| c.component_index.is_some_and(|i| i >= first_module_len)),
            "the second module publishes global indices"
        );
        // Published indices are distinct and in range; unused geometry cells are
        // never published (they would carry `None`).
        let mut idx: Vec<usize> = spec
            .cells
            .iter()
            .filter_map(|c| c.component_index)
            .collect();
        let published = idx.len();
        idx.sort_unstable();
        idx.dedup();
        assert_eq!(idx.len(), published, "component indices are distinct");
        assert!(idx.iter().all(|&i| i < patch.hw_components.len()));
    }

    #[test]
    fn physical_spec_marks_the_selected_element_cell_highlighted() {
        // droid_tui-8ia: the shared element selection keeps the clicked module
        // UI cell visibly marked after the pointer leaves (hover alone used to
        // be the only highlight source).
        let mut app = app_with_patch("fixtures/source_navigation.ini");
        let idx = app
            .patch
            .as_ref()
            .unwrap()
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .expect("fixture declares B1.1");
        let before = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let cell = before
            .cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .expect("B1.1 cell");
        assert!(!cell.highlighted, "nothing selected or hovered yet");

        app.select_component(String::from("B1.1"));
        let after = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let cell = after
            .cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .expect("B1.1 cell");
        assert!(
            cell.highlighted,
            "the selected element stays marked after the pointer leaves"
        );
    }

    #[test]
    fn physical_spec_marks_the_selected_circuits_cells_highlighted() {
        // droid_tui-8ia reverse direction: clicking a graph node sets
        // `selected_circuit`; `circuit_hw_token_indices` maps it back to the
        // module UI cells, which the spec must mark (it was computed but unused
        // before this change).
        let mut app = app_with_patch("fixtures/source_navigation.ini");
        app.open_view(crate::app::ViewType::Graph);
        let node = app
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.circuit == "button")
            .expect("fixture has a button circuit")
            .id
            .clone();
        app.select_circuit(node);
        let indices = app.circuit_hw_token_indices();
        assert!(
            !indices.is_empty(),
            "the button section names hardware tokens"
        );
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        for i in indices {
            let cell = spec
                .cells
                .iter()
                .find(|c| c.component_index == Some(i))
                .unwrap_or_else(|| panic!("cell for hardware index {i}"));
            assert!(cell.highlighted, "circuit cell {i} is highlighted");
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
            led_capability: Some(crate::patch::LedCapability::Rgb),
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
        painted_output(spec).0
    }

    /// Headless paint of `spec` into a default 800×600 ui: the emitted text
    /// labels and `Shape::Rect` rects, mirroring the gallery-row assertions
    /// for the physical surface.
    fn painted_output(spec: &PhysicalSpec) -> (Vec<String>, Vec<Rect>) {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_physical(ui, ui.max_rect(), Some(spec));
        });
        let mut labels = Vec::new();
        let mut rects = Vec::new();
        for cs in &full_output.shapes {
            match &cs.shape {
                egui::epaint::Shape::Text(t) => labels.push(t.galley.text().to_string()),
                egui::epaint::Shape::Rect(r) => rects.push(r.rect),
                _ => {}
            }
        }
        // The real loop applies `textures_delta` to the wgpu texture manager
        // (mod.rs); a headless test drops the output, so release it first
        // (epaint panics on dropping unapplied deltas).
        full_output.textures_delta.clear();
        (labels, rects)
    }

    /// One headless `Context::run_ui` pass over `paint`: every emitted text
    /// shape as `(text, font, galley width)` so tests can assert on the exact
    /// font a label was painted with, not just its content.
    fn headless_texts(mut paint: impl FnMut(&mut egui::Ui)) -> Vec<(String, egui::FontId, f32)> {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| paint(ui));
        let mut texts = Vec::new();
        for cs in &full_output.shapes {
            if let egui::epaint::Shape::Text(t) = &cs.shape {
                let font = t.galley.job.sections[0].format.font_id.clone();
                texts.push((t.galley.text().to_string(), font, t.galley.size().x));
            }
        }
        full_output.textures_delta.clear();
        texts
    }

    /// `headless_texts` over a whole spec paint (`paint_physical`).
    fn painted_texts(spec: &PhysicalSpec) -> Vec<(String, egui::FontId, f32)> {
        headless_texts(|ui| {
            paint_physical(ui, ui.max_rect(), Some(spec));
        })
    }

    /// `headless_texts` over a single cell paint (`paint_cell`, full
    /// presentation), for the label font/ellipsize/omission tests.
    fn painted_cell(cell: &CellSpec) -> Vec<(String, egui::FontId, f32)> {
        headless_texts(|ui| {
            let painter = ui.painter();
            paint_cell(painter, cell, false, false);
        })
    }

    /// Headless `paint_cell` rect shapes as `(rect, fill, stroke width,
    /// stroke color)` so contrast-chrome tests can tell filled rects from
    /// stroked outlines (fader-led-contrast 3.1).
    fn painted_cell_rects(
        cell: &CellSpec,
        paused: bool,
    ) -> Vec<(Rect, egui::Color32, f32, egui::Color32)> {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            let painter = ui.painter();
            paint_cell(painter, cell, false, paused);
        });
        let mut rects = Vec::new();
        for cs in &full_output.shapes {
            if let egui::epaint::Shape::Rect(r) = &cs.shape {
                rects.push((r.rect, r.fill, r.stroke.width, r.stroke.color));
            }
        }
        full_output.textures_delta.clear();
        rects
    }

    /// A fader cell for the contrast tests: track + strip + marker + label.
    fn fader_spec(value: f32) -> CellSpec {
        CellSpec {
            rect: Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(40.0, 100.0)),
            glyph: String::new(),
            label: "F".to_string(),
            state_text: "50%".to_string(),
            color: crate::theme::active().knob,
            is_fader: true,
            fader_value: value,
            component_index: Some(0),
            mark: PortMark::Cell,
            highlighted: false,
            shift_color: None,
            modifier_wash: None,
            dimmed: false,
            kind: ComponentKind::Knob,
            led_state: None,
            led_rgb: None,
        }
    }

    #[test]
    fn fader_paints_slot_outline_and_value_marker() {
        let rects = painted_cell_rects(&fader_spec(0.5), false);
        // Slot outline: 1 px stroke around the track (left edge strip).
        assert!(
            rects.iter().any(|(r, _, w, c)| *w == 1.0
                && *c == rgb(crate::theme::active().fader_slot)
                && r.min.x == 10.0),
            "slot outline stroke present: {rects:?}"
        );
        // Value marker: thin filled rect in the strip tone at fill height.
        assert!(
            rects.iter().any(|(r, fill, w, _)| *w == 0.0
                && *fill == rgb(crate::theme::active().fader_led_bar)
                && r.height() <= 2.0
                && r.height() >= 1.0),
            "value marker fill present: {rects:?}"
        );
    }

    #[test]
    fn fader_strip_stays_bright_while_paused() {
        let strip = rgb(crate::theme::active().fader_led_bar);
        for paused in [false, true] {
            let rects = painted_cell_rects(&fader_spec(0.5), paused);
            assert!(
                rects
                    .iter()
                    .any(|(_, fill, w, _)| *w == 0.0 && *fill == strip),
                "lit strip full-bright (paused={paused}): {rects:?}"
            );
        }
        // The unlit track still dims while paused.
        let dimmed_track = dim(rgb(crate::theme::active().muted), true);
        let rects = painted_cell_rects(&fader_spec(0.5), true);
        assert!(
            rects.iter().any(|(_, fill, _, _)| *fill == dimmed_track),
            "unlit track dimmed while paused: {rects:?}"
        );
    }

    #[test]
    fn led_paints_ring_with_brightness_floor() {
        let mut cell = cell_spec(
            Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(40.0, 40.0)),
            "B",
        );
        cell.led_state = Some(0.0);
        let rects = painted_cell_rects(&cell, false);
        // Ring: 1 px stroke in the ring tone.
        assert!(
            rects
                .iter()
                .any(|(_, _, w, c)| *w == 1.0 && *c == rgb(crate::theme::active().led_ring)),
            "LED ring stroke present: {rects:?}"
        );
        // Core: filled inset at the 25 % floor (63/255), not black.
        let floor = egui::Color32::from_rgb(63, 63, 63);
        assert!(
            rects
                .iter()
                .any(|(_, fill, w, _)| *w == 0.0 && *fill == floor),
            "LED core at brightness floor: {rects:?}"
        );
    }

    /// A compact full-presentation button cell for the label tests: the
    /// fixed-size label path, no interaction flags.
    fn cell_spec(rect: Rect, label: &str) -> CellSpec {
        CellSpec {
            rect,
            glyph: "\u{25CF}".to_string(),
            label: label.to_string(),
            state_text: "ON".to_string(),
            color: crate::theme::active().button,
            is_fader: false,
            fader_value: 0.0,
            component_index: Some(0),
            mark: PortMark::Cell,
            highlighted: false,
            shift_color: None,
            modifier_wash: None,
            dimmed: false,
            kind: ComponentKind::Button,
            led_state: None,
            led_rgb: None,
        }
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

    /// label-guess-and-screen-scale, task 2.2: the cell label renders at the
    /// fixed `CELL_LABEL_SIZE` at every zoom preset — the font never derives
    /// from the (zoom-scaled) cell height — and every preset still shows a
    /// label next to the glyph.
    #[test]
    fn cell_label_font_is_identical_across_zoom_presets() {
        let mut sizes = Vec::new();
        for &zoom in &[0.75f64, 1.0, 1.5, 2.0] {
            let spec = spec_at_zoom(zoom);
            let rows: Vec<(String, egui::FontId, f32)> = painted_texts(&spec)
                .into_iter()
                .filter(|(_, font, _)| font.family == egui::FontFamily::Monospace)
                .collect();
            assert!(!rows.is_empty(), "cell first rows drawn at zoom {zoom}");
            assert!(
                rows.iter().any(|(text, _, _)| text.contains(' ')),
                "a label joins the glyph row at zoom {zoom}: {rows:?}"
            );
            for (text, font, _) in &rows {
                assert_eq!(
                    font.size, CELL_LABEL_SIZE,
                    "fixed label font at zoom {zoom}: {text:?} painted at {} pt",
                    font.size
                );
            }
            sizes.extend(rows.iter().map(|(_, font, _)| font.size));
        }
        // One size across all four presets, not merely four stable sizes.
        assert!(
            sizes.iter().all(|&s| s == CELL_LABEL_SIZE),
            "identical font across zoom presets: {sizes:?}"
        );
    }

    /// label-guess-and-screen-scale, task 2.2: a cell narrower than one
    /// character of `CELL_LABEL_SIZE` renders the glyph but no label text.
    #[test]
    fn cell_narrower_than_one_character_hides_the_label() {
        let char_w = MONO_ADVANCE * CELL_LABEL_SIZE;
        let rect = Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::new(char_w * 0.9, 24.0));
        let texts = painted_cell(&cell_spec(rect, "B1.1"));
        // The glyph row stays (state glyph always), but carries no label —
        // assert on the monospace first row so the second-row state text
        // (which has its own ellipsis rule) cannot satisfy the check.
        let rows: Vec<&str> = texts
            .iter()
            .filter(|(_, font, _)| font.family == egui::FontFamily::Monospace)
            .map(|(text, _, _)| text.as_str())
            .collect();
        assert!(
            rows.iter().any(|row| row.contains('\u{25CF}')),
            "glyph row drawn: {texts:?}"
        );
        assert!(
            rows.iter()
                .all(|row| !row.contains(' ') && !row.contains("B1.1")),
            "no label below one character of the label font: {texts:?}"
        );
        assert!(
            !texts.iter().any(|(t, _, _)| t.contains("B1.1")),
            "no label text anywhere: {texts:?}"
        );
    }

    /// label-guess-and-screen-scale, task 2.2: a narrow-but-wide-enough cell
    /// still shows the glyph row with the label ellipsized to the cell width,
    /// never the full label and never an overflowing row.
    #[test]
    fn cell_label_ellipsizes_to_the_cell_width() {
        let char_w = MONO_ADVANCE * CELL_LABEL_SIZE;
        // Room for the glyph, the separating space, and a few label
        // characters — but far short of the whole label.
        let rect = Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::new(char_w * 8.0, 24.0));
        let texts = painted_cell(&cell_spec(rect, "averylonglabel"));
        let (row, _, width) = texts
            .iter()
            .find(|(_, font, _)| font.family == egui::FontFamily::Monospace)
            .expect("glyph row drawn");
        assert!(row.contains('\u{2026}'), "ellipsized: {row:?}");
        assert!(
            !row.contains("averylonglabel"),
            "not the full label: {row:?}"
        );
        assert!(
            *width <= rect.width() + 1.0,
            "row fits the cell width: {width} pt > {} pt ({row:?})",
            rect.width()
        );
    }

    #[test]
    fn rack_geometry_derives_db8e_band_from_b_grid_top() {
        let (rack, chain) = db8e_fixture();
        let geom = rack_geometry(&rack, &chain, &mapping(), 10.0);
        assert_eq!(geom.db8e_bands.len(), 1);
        let (mi, band) = geom.db8e_bands[0];
        assert_eq!(mi, 0);
        let module = geom.module_rects[0].1;
        // Band = module inset 1 pt; height = (B-grid top 38 mm / 128.5 mm) of
        // the module height, rounded, minus the 2 border strokes.
        let expect_h = (module.height() * 38.0 / 128.5).round() - 2.0;
        assert!(
            (band.height() - expect_h).abs() < 1e-3,
            "h {}",
            band.height()
        );
        assert_eq!(band.min, module.min + egui::vec2(1.0, 1.0));
        assert!((band.max.x - (module.max.x - 1.0)).abs() < 1e-3);
        // The band bottom sits at/above the B-grid top (y_mm 38 → 114 pt).
        assert!(band.max.y <= 114.0 + 1e-3, "y {}", band.max.y);
    }

    #[test]
    fn paint_draws_db8e_display_band_and_state() {
        for skeleton in [false, true] {
            let spec = db8e_spec(skeleton, false);
            let (labels, rects) = painted_output(&spec);
            // Display state derived from the layout (db8e present, no
            // mismatch marker → "connected"), ellipsized to the band width.
            let band = spec.db8e_bands[0].rect;
            let expected = band_text("connected", band.width());
            assert!(
                labels.contains(&expected),
                "db8e state text drawn: {labels:?}"
            );
            // A bordered rect inside the module's upper band (above the B-grid).
            let module = spec.modules[0].rect;
            assert!(
                rects.iter().any(|r| {
                    r.min.x > module.min.x
                        && r.max.x < module.max.x
                        && r.min.y <= module.min.y + module.height() * 0.4
                }),
                "display band rect drawn: {rects:?}"
            );
        }
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
            frame = paint_physical(ui, ui.max_rect(), None);
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
            frame = ui.ctx().input(|i| physical_frame(i, 10.0, ui.max_rect()));
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

    // -- performance view (performance-view 1.3) ---------------------------

    fn perf_pane() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    fn perf_spec() -> PhysicalSpec {
        let mut app = app_with_patch("fixtures/own_buttons.ini");
        app.showing_performance = true;
        physical_spec(&app, perf_pane()).expect("performance spec")
    }

    fn rects_disjoint(a: &Rect, b: &Rect) -> bool {
        a.min.x >= b.max.x || b.min.x >= a.max.x || a.min.y >= b.max.y || b.min.y >= a.max.y
    }

    #[test]
    fn performance_overlay_absent_by_default_and_placed_clear_when_flag_on() {
        let app = app_with_patch("fixtures/own_buttons.ini");
        let plain = physical_spec(&app, perf_pane()).expect("plain spec");
        assert!(plain.performance.is_none(), "no overlay without the flag");
        let spec = perf_spec();
        let overlay = spec.performance.as_ref().expect("overlay when flag on");
        assert!(
            !overlay.callouts.is_empty(),
            "every labelled element gets a callout"
        );
        for c in &overlay.callouts {
            assert!(
                rects_disjoint(&c.label_rect, &overlay.rack_rect),
                "callout '{}' enters the rack rect: {:?}",
                c.label,
                c.label_rect
            );
        }
        for (i, a) in overlay.callouts.iter().enumerate() {
            for b in &overlay.callouts[i + 1..] {
                assert!(
                    rects_disjoint(&a.label_rect, &b.label_rect),
                    "callouts '{}' and '{}' overlap",
                    a.label,
                    b.label
                );
            }
        }
    }

    #[test]
    fn performance_rack_is_compact_and_centered() {
        let spec = perf_spec();
        // Half-linear fit → at most half the pane in each dimension (~1/4 area).
        assert!(
            spec.case_rect.width() <= 400.0 + 1.0,
            "rack width compact: {}",
            spec.case_rect.width()
        );
        assert!(
            spec.case_rect.height() <= 300.0 + 1.0,
            "rack height compact: {}",
            spec.case_rect.height()
        );
        let center = spec.case_rect.center();
        assert!(
            (center.x - 400.0).abs() <= 2.0 && (center.y - 300.0).abs() <= 2.0,
            "rack centered on the pane: {center:?}"
        );
    }

    fn nav_patch() -> crate::patch::Patch {
        crate::patch::Patch::from_ini_file(std::path::Path::new("fixtures/source_navigation.ini"))
            .unwrap()
    }

    #[test]
    fn owning_circuit_node_resolves_first_occurrence_section() {
        let patch = nav_patch();
        // B1.1 first occurs in the first [button] section (instance 0).
        assert_eq!(
            owning_circuit_node(&patch, "B1.1"),
            Some(NodeId::circuit("button", 0))
        );
        assert_eq!(owning_circuit_node(&patch, "B9.9"), None);
    }

    #[test]
    fn performance_headline_resolves_store_then_guess_then_derived() {
        let patch = nav_patch();
        let mut app = App::new();
        let hw_store: std::collections::HashMap<String, std::collections::BTreeMap<u8, String>> =
            std::collections::HashMap::new();
        let circuit_store: std::collections::HashMap<NodeId, String> =
            std::collections::HashMap::new();
        let resolve =
            |app: &App,
             hw: &std::collections::HashMap<String, std::collections::BTreeMap<u8, String>>,
             fallback: &str| {
                let ctx = HeadlineCtx {
                    patch: &patch,
                    app,
                    shift: 1,
                    layers_enabled: true,
                    max_shift_layer: 4,
                    hw_store: hw,
                    circuit_store: &circuit_store,
                };
                performance_headline(&ctx, "B1.1", fallback)
            };
        let fallback = patch.display_label("B1.1", 1, true, 4, &hw_store);
        // No store, no guess → derived fallback.
        assert_eq!(resolve(&app, &hw_store, &fallback), fallback);
        // Guess wins over derived.
        app.guessed_labels
            .insert(NodeId::circuit("button", 0), "DirKey".into());
        assert_eq!(resolve(&app, &hw_store, &fallback), "DirKey");
        // Explicit store wins over guess.
        let mut hw: std::collections::HashMap<String, std::collections::BTreeMap<u8, String>> =
            std::collections::HashMap::new();
        hw.insert(
            "B1.1".to_string(),
            [(1u8, "Cut".to_string())].into_iter().collect(),
        );
        assert_eq!(resolve(&app, &hw, &fallback), "Cut");
        // A token with no occurrence falls back to derived.
        let derived = patch.display_label("B9.9", 1, true, 4, &hw_store);
        let ctx = HeadlineCtx {
            patch: &patch,
            app: &app,
            shift: 1,
            layers_enabled: true,
            max_shift_layer: 4,
            hw_store: &hw_store,
            circuit_store: &circuit_store,
        };
        assert_eq!(performance_headline(&ctx, "B9.9", &derived), derived);
    }

    fn painted_line_count(spec: &PhysicalSpec) -> usize {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_physical(ui, ui.max_rect(), Some(spec));
        });
        let n = full_output
            .shapes
            .iter()
            .filter(|cs| matches!(cs.shape, egui::epaint::Shape::LineSegment { .. }))
            .count();
        full_output.textures_delta.clear();
        n
    }

    #[test]
    fn performance_paint_draws_leader_lines_and_big_headlines() {
        let spec = perf_spec();
        let overlay = spec.performance.as_ref().expect("overlay");
        // One leader line per callout (fold bars add a few more).
        assert!(
            painted_line_count(&spec) >= overlay.callouts.len(),
            "each callout gets a leader line"
        );
        let texts = painted_texts(&spec);
        for c in &overlay.callouts {
            assert!(
                texts.iter().any(|(t, font, _)| t == &c.label
                    && *font == egui::FontId::monospace(CALLOUT_LABEL_SIZE)),
                "headline '{}' paints big",
                c.label
            );
        }
    }

    #[test]
    fn performance_callout_state_follows_live_component_state() {
        let mut app = app_with_patch("fixtures/own_buttons.ini");
        app.showing_performance = true;
        // First resting button, matched by host-cell anchor (headline text
        // need not be unique across elements).
        let token = {
            let patch = app.patch.as_ref().expect("patch");
            patch
                .hw_components
                .iter()
                .find(|c| c.kind == ComponentKind::Button && matches!(c.state, ComponentState::Off))
                .map(|c| c.id.clone())
                .expect("a resting button")
        };
        // Headline doubles as the matcher: with no store override and no
        // open graph (no guesses) it equals the plain display label, unique
        // per token in this fixture; the anchor check pins the host cell.
        let headline = {
            let patch = app.patch.as_ref().expect("patch");
            patch.display_label(&token, 1, true, 4, &app.current_hw_store())
        };
        let state_of = |app: &App| -> String {
            let spec = physical_spec(app, perf_pane()).expect("spec");
            let overlay = spec.performance.expect("overlay");
            let hw_idx = app
                .patch
                .as_ref()
                .expect("patch")
                .hw_components
                .iter()
                .position(|c| c.id == token)
                .expect("token");
            let cell_rect = spec
                .cells
                .iter()
                .find(|c| c.component_index == Some(hw_idx))
                .expect("cell")
                .rect;
            overlay
                .callouts
                .iter()
                .find(|c| {
                    c.label == headline
                        && c.anchor.x >= cell_rect.min.x
                        && c.anchor.x <= cell_rect.max.x
                        && c.anchor.y >= cell_rect.min.y
                        && c.anchor.y <= cell_rect.max.y
                })
                .unwrap_or_else(|| panic!("callout for {token}"))
                .state_text
                .clone()
        };
        let before = state_of(&app);
        app.patch
            .as_mut()
            .expect("patch")
            .hw_components
            .iter_mut()
            .find(|c| c.id == token)
            .expect("token")
            .state = ComponentState::On;
        let after = state_of(&app);
        assert_ne!(before, after, "callout state reads live ComponentState");
        assert!(after.contains("ON"), "On state shows: {after}");
    }
}
