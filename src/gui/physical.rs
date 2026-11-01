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

/// Monospace advance as a fraction of the font size (egui's monospace face is
/// close to 0.6 em — mirrors `gui::graph`'s `MONO_ADVANCE`), used to measure
/// callouts and to budget the state row. Cell labels no longer derive a budget
/// from it: every label size and every line comes from
/// [`crate::physical::label_layout`] (module-ui-label-fit D1).
const MONO_ADVANCE: f32 = 0.6;
/// Left inset of a cell's label lines, in points. Mirrors `physical`'s
/// `LABEL_INSET_X`, which measured the text box the layout was fitted against.
const LABEL_INSET_X: f32 = 2.0;
/// Fader track fraction of the cell width, its ceiling and its gutter
/// (module-ui-label-fit D5). Mirrors `physical`'s `FADER_TRACK_WIDTH_FRACTION`,
/// `FADER_TRACK_WIDTH_MAX` and `FADER_TRACK_GUTTER`, so the painter narrows the
/// track to exactly the column the model measured the label against.
const FADER_TRACK_WIDTH_FRACTION: f32 = 0.4;
const FADER_TRACK_WIDTH_MAX: f32 = 10.0;
const FADER_TRACK_GUTTER: f32 = 3.0;
/// Compact state-glyph marker (D3): `h × 0.28` points, clamped to the 6–9 pt
/// band, and skipped outright when the unclamped size falls below the 6 pt
/// legibility floor — the marker never shrinks past it.
const STATE_MARKER_SCALE: f32 = 0.28;
const STATE_MARKER_MIN: f32 = 6.0;
const STATE_MARKER_MAX: f32 = 9.0;

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
///
/// A button-kind cell on a controller whose LED lights the button face
/// (button-led-composition D2) carries a `shape` instead of a glyph: the
/// outline carries the button state and the folded LED is the inner fill.
/// `None` keeps today's glyph path for every other element kind and
/// controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellShape {
    /// Round face (P2B8, B32): circle outline + concentric disc fill.
    Round,
    /// Rectangular face (M4 touch plate): rect outline + inner rect fill.
    Rect,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellSpec {
    pub rect: Rect,
    pub glyph: String,
    pub label: String,
    pub state_text: String,
    pub color: Color,
    /// The colour the element's label and its callout headline paint in. Kept
    /// apart from [`CellSpec::color`] because a composed face's `color` is the
    /// *state* outline (`muted` when off), and a label inheriting that outline
    /// read as dark grey on every off button.
    pub label_color: Color,
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
    /// Composed-face discriminant (button-led-composition D2): `Some` on
    /// button-kind cells of P2B8/B32 (round) and M4 (rect), `None` elsewhere.
    pub shape: Option<CellShape>,
    /// The fitted label of this cell (module-ui-label-fit D1), computed once by
    /// [`cell_label_layout`] when the spec is built: `None` when the label is
    /// empty or no ladder step can draw a character of it in this cell's own
    /// text box. The painter draws [`crate::physical::LabelLayout::lines`]
    /// verbatim, so the cells and the annotation layer can never disagree about
    /// what the cell shows.
    pub label_layout: Option<crate::physical::LabelLayout>,
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
    /// Performance-view overlay (compact rack + exploded callouts), present
    /// exactly when `App.showing_performance` was on at spec-build time.
    pub performance: Option<PerformanceOverlay>,
    /// Label-presentation annotations (module-ui-labels D10): side-bound
    /// callouts with leader lines, outside the rack, holding the labels the
    /// cells themselves cannot show. Decorative — they publish no hit rect and
    /// never touch `cells` or the geometry. Empty when nothing is annotated
    /// (`off`, `auto` with nothing clipped, budget-dropped set empty).
    pub annotations: Vec<CalloutSpec>,
    /// Hover reveal (module-ui-labels D11): the full label plus state of the
    /// element under the pointer, anchored to its cell and flipped to stay
    /// inside the pane. `None` with no hovered element.
    pub hover: Option<CalloutSpec>,
    /// Skeleton presentation: module outlines + cell markers only.
    pub skeleton: bool,
    /// Global processing pause: everything renders dimmed.
    pub paused: bool,
}

/// The frame's physical-view input, reported back for the loop to apply to
/// `App` (mirrors `WindowFrame`): middle-drag pan in points, the wheel's
/// disambiguated intent (value adjust on an adjustable element, else zoom about
/// the cursor), and a plain `s` skeleton toggle.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PhysicalFrame {
    /// Pan delta in points (`physical_offset` units; the mapping yields points).
    pub pan_delta: (f32, f32),
    /// Wheel zoom: factor plus the cursor anchor in points.
    pub zoom: Option<(f32, (f32, f32))>,
    /// Wheel value adjust (module-ui-labels D7): the `patch.hw_components`
    /// index of the adjustable element under the pointer, plus the signed
    /// `±WHEEL_VALUE_STEP` to apply. Set instead of `zoom`, never beside it,
    /// unless `Ctrl` forces the zoom.
    pub value_adjust: Option<(usize, f32)>,
    /// `s` pressed this frame (no modifiers): toggle skeleton presentation.
    pub skeleton_toggle: bool,
}

/// One wheel notch over an adjustable element: the terminal path's
/// `adjust_value(comp, ±0.05)` step (module-ui-labels D7).
const WHEEL_VALUE_STEP: f32 = 0.05;

/// The fold is a per-controller rule (button-led-composition D2): only
/// button-kind cells on P2B8/B32 (round face) and M4 (rect touch plate)
/// fold their LED into the face. Every other controller keeps the glyph
/// rendering and standalone LED cells.
fn fold_shape_for_controller(controller: &str) -> Option<CellShape> {
    match controller.to_ascii_lowercase().as_str() {
        "p2b8" => Some(CellShape::Round),
        "b32" | "notebuttons" => Some(CellShape::Round),
        "m4" | "motorfader" => Some(CellShape::Rect),
        _ => None,
    }
}

/// The knob position a fader face draws (module-ui-label-fit D5): the element's
/// own state, read the way the terminal path reads it — a `Value` is the patch's
/// 0..1 position, an `On` button-like state is fully raised, and every state
/// without a position (off, an LED's brightness, a CV ring) leaves the track
/// empty rather than inventing a mid-scale value.
fn fader_value_for(state: &crate::patch::ComponentState) -> f32 {
    use crate::patch::ComponentState;
    match state {
        ComponentState::Value(v) => v.clamp(0.0, 1.0),
        ComponentState::On => 1.0,
        ComponentState::Off | ComponentState::Active | ComponentState::Led { .. } => 0.0,
    }
}

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
    let pane_dx = pane.min.x as f64;
    let pane_dy = pane.min.y as f64;
    let normal_mapping = || {
        ScreenMapping::new(
            crate::physical::PHYSICAL_POINTS_PER_MM,
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
    let geom = rack_geometry(&rack, &chain, &m);
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

    // One binding source (button-led-composition D3): the shared
    // `folded_led_for_element` resolver feeds both the fill and the
    // suppression, so painter and handler cannot disagree.
    for module in chain.modules.iter() {
        for comp in &module.components {
            if comp.kind != crate::patch::ComponentKind::Button {
                continue;
            }
            if fold_shape_for_controller(&comp.controller).is_none() {
                continue;
            }
            if let Some(led_token) = crate::patch::folded_led_for_element(comp) {
                folded_leds.insert(led_token);
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

        // Suppression (button-led-composition D4): a folded LED publishes no
        // cell, so it is neither painted nor hit-testable. The skeleton and
        // the full render share this filter, so they stay coincident — and
        // `paint_panes` republishes `component_rects` from these cells, so
        // the LED hit rects drop with no second change.
        if ci < module.components.len() && folded_leds.contains(&module.components[ci].id) {
            continue;
        }

        // Fold suppression (unreferenced-elements D2): an unaddressed L cell
        // on a fold-rule controller never renders standalone — its face (or
        // dimmed unaddressed face) already shows the floor fill. Recover the
        // token via the cell label and apply the same per-controller rule.
        if ci >= module.components.len() {
            let ordinal = ci - module.components.len();
            if let Some(unaddr) = module.unaddressed_cells().get(ordinal) {
                if unaddr.kind_letter == Some('L')
                    && fold_shape_for_controller(&module.geometry_key).is_some()
                {
                    continue;
                }
            }
        }
        // ci might be out of bounds for patch components if it's an unused element
        // In that case, we still render the cell but dimmed and unlabelled
        let (
            glyph,
            state_text,
            color,
            label,
            kind,
            is_fader,
            fader_value,
            led_state,
            led_rgb,
            shape,
            label_color,
        ) = if ci < module.components.len() {
            let comp = &module.components[ci];
            let shape = if comp.kind == crate::patch::ComponentKind::Button {
                fold_shape_for_controller(&comp.controller)
            } else {
                None
            };
            let is_shift_active = app
                .active_shift
                .is_some_and(|g| comp.shift_group == Some(g));
            // The geometry cell is the one source for both faceplate facts: the
            // `kind` the switch glyph path reads, and the family that says this
            // element is a fader. P8S8/M4 sliders are addressed with `P`
            // registers but live in the geometry's `F` family, so the flag
            // cannot come from the token's own letter (module-ui-label-fit D5);
            // the label fit already classifies the same family as
            // `CellTextPath::Fader`, and the painter now agrees instead of
            // drawing every fader as a compact cell.
            let element_cell = chain.cell_for(mi, &comp.id);
            let switch_kind = element_cell.and_then(|c| c.kind.as_deref());
            let is_fader = element_cell.is_some_and(|c| c.family == "F");
            let fader_value = fader_value_for(&comp.state);
            let (glyph, state_text, color) =
                cell_visuals(comp, is_shift_active, is_fader, switch_kind);
            // The folded LED (if any) drives the inner fill.
            let folded = shape
                .is_some()
                .then(|| crate::patch::folded_led_for_element(comp))
                .flatten();
            let folded_comp = folded
                .as_deref()
                .and_then(|led| module.components.iter().find(|c| c.id == *led));
            let led_state = folded_comp.map(|c| match &c.state {
                crate::patch::ComponentState::Value(v) => *v,
                crate::patch::ComponentState::On => 1.0,
                crate::patch::ComponentState::Off => 0.0,
                crate::patch::ComponentState::Led { value, .. } => *value,
                crate::patch::ComponentState::Active => 1.0,
            });
            let led_rgb = folded_comp.and_then(|c| match &c.state {
                crate::patch::ComponentState::Led { value: _, rgb } => *rgb,
                _ => None,
            });
            // Use shift-aware label resolver
            let shift = app.active_shift.map_or(1, |g| g as u8);
            let layers_enabled = app.labels.layers_enabled;
            let max_shift_layer = app.labels.max_shift_layer;
            let hw_store = app.current_hw_store();
            let label =
                patch.display_label(&comp.id, shift, layers_enabled, max_shift_layer, &hw_store);
            // A composed face carries the state on its outline (D6), so
            // the glyph and the state-text row go away; the outline color
            // is the state channel (off `muted`, on the button token).
            if shape.is_some() {
                let on = matches!(comp.state, crate::patch::ComponentState::On);
                let outline = if on {
                    color
                } else {
                    crate::theme::active().muted
                };
                (
                    String::new(),
                    String::new(),
                    outline,
                    label,
                    comp.kind,
                    false,
                    0.0,
                    led_state,
                    led_rgb,
                    shape,
                    // The label keeps the element's own token: the shape's
                    // outline alone carries the state, so an off face does not
                    // drag its label down to `muted`.
                    color,
                )
            } else {
                (
                    glyph,
                    state_text,
                    color,
                    label,
                    comp.kind,
                    is_fader,
                    fader_value,
                    led_state,
                    led_rgb,
                    shape,
                    color,
                )
            }
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
                None,
                color,
            )
        };

        // Shift slot mirrors the `cell_visuals` group→token match; unaddressed
        // cells (`ci >= len`) are always `None` (D2).
        let shift_color = if ci < module.components.len() {
            let comp = &module.components[ci];
            if app
                .active_shift
                .is_some_and(|g| comp.shift_group == Some(g))
            {
                let t = crate::theme::active();
                match comp.shift_group {
                    Some(ShiftGroup::Group1) => Some(t.shift1),
                    Some(ShiftGroup::Group2) => Some(t.shift2),
                    Some(ShiftGroup::Group3) => Some(t.shift3),
                    Some(ShiftGroup::Group4) => Some(t.shift4),
                    None => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        // The active shift tints the label too, on every kind (the shift slot
        // is built for all of them, not only buttons).
        let label_color = shift_color.unwrap_or(label_color);
        let is_dimmed = label.is_empty(); // unused elements are dimmed
        let mut cell = CellSpec {
            rect,
            glyph,
            label,
            state_text,
            color,
            label_color,
            is_fader,
            fader_value,
            component_index,
            mark,
            highlighted: component_index.is_some_and(|i| {
                app.hovered_component == Some(i)
                    || selected_circuit_cells.contains(&i)
                    || app.selected_component.as_deref() == Some(module.components[ci].id.as_str())
            }),
            shift_color,
            modifier_wash: None,
            dimmed: is_dimmed,
            kind,
            led_state,
            led_rgb,
            shape,
            label_layout: None,
        };
        // D1: the cell's own box answers once what its label looks like — the
        // painter draws these lines and the annotation filter reads the same
        // `None` instead of re-deriving a fit test that could drift.
        cell.label_layout = cell_label_layout(&cell);
        cells.push(cell);
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
    // Label annotations (D10): `off` draws none, `auto` annotates exactly the
    // addressed cells whose label has no in-cell layout (the restated rule of
    // module-ui-label-fit D9), `on` every addressed label. The performance view
    // already explodes every label big around a compact rack, so it carries no
    // second annotation layer. Candidates are the same cells the painter
    // consumes (rect + label + layout).
    let annotations = if app.showing_performance {
        Vec::new()
    } else {
        // `auto` only annotates labels the user actually wrote: a derived token
        // name ("Button B1.3") is mechanical, so annotating every derived name a
        // narrow cell ellipsizes would flood the field with noise. `on` draws
        // every addressed label on request, derived names included.
        let shift = app.active_shift.map_or(1, |g| g as u8);
        let layers_enabled = app.labels.layers_enabled;
        let max_shift_layer = app.labels.max_shift_layer;
        let hw_store = app.current_hw_store();
        let explicitly_labelled = |cell: &CellSpec| -> bool {
            cell.component_index
                .and_then(|i| patch.hw_components.get(i))
                .is_some_and(|comp| {
                    patch
                        .explicit_hw_label(
                            &comp.id,
                            shift,
                            layers_enabled,
                            max_shift_layer,
                            &hw_store,
                        )
                        .is_some()
                })
        };
        let sources: Vec<CellSpec> = cells
            .iter()
            .filter(|cell| {
                cell.component_index.is_some()
                    && !cell.label.is_empty()
                    && match app.label_presentation {
                        crate::config::LabelPresentation::Off => false,
                        crate::config::LabelPresentation::Auto => {
                            cell.label_layout.is_none() && explicitly_labelled(cell)
                        }
                        crate::config::LabelPresentation::On => true,
                    }
            })
            .map(|cell| with_resolved_state(cell, patch))
            .collect();
        let candidates: Vec<&CellSpec> = sources.iter().collect();
        budget_annotations(pane, geom.case_rect, &candidates).callouts
    };
    // Hover reveal (D11): the full label plus state of the element under the
    // pointer, anchored to its cell and drawn in every presentation mode and in
    // the skeleton. `physical_spec` takes no pointer, so the handler-published
    // `hovered_component` is the single input; the performance view already
    // explodes every label and draws no second callout.
    let hover = if app.showing_performance {
        None
    } else {
        app.hovered_component
            .and_then(|index| {
                cells
                    .iter()
                    .find(|cell| cell.component_index == Some(index))
            })
            .map(|cell| with_resolved_state(cell, patch))
            .and_then(|cell| hover_callout(pane, &cell))
    };
    Some(PhysicalSpec {
        performance,
        annotations,
        hover,
        background: crate::theme::active().graph_canvas_bg,
        case_rect: geom.case_rect,
        mounts: geom.mounts,
        fold_bars: geom.fold_bars,
        modules,
        cells,
        db8e_bands,
        grid_lines: mm_grid_lines(&m, rack.total_width_mm, rack.total_height_mm, 20.0),
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
    let points_per_mm = crate::physical::PHYSICAL_POINTS_PER_MM;
    let pane_w = pane.width() as f64;
    let pane_h = pane.height() as f64;
    if pane_w <= 0.0 || pane_h <= 0.0 {
        return None;
    }
    if rack.total_width_mm <= 0.0 || rack.total_height_mm <= 0.0 {
        return None;
    }
    let fit = (pane_w / (rack.total_width_mm * points_per_mm))
        .min(pane_h / (rack.total_height_mm * points_per_mm));
    if !fit.is_finite() || fit <= 0.0 {
        return None;
    }
    let zoom = fit * 0.5;
    // Pane-relative points center on the pane middle; the pane origin folds in
    // exactly like the normal mapping so points land window-absolute.
    let offset_x =
        (rack.total_width_mm * 0.5) * points_per_mm * zoom - pane.min.x as f64 - pane_w * 0.5;
    let offset_y =
        (rack.total_height_mm * 0.5) * points_per_mm * zoom - pane.min.y as f64 - pane_h * 0.5;
    Some(ScreenMapping::new(points_per_mm, zoom, offset_x, offset_y))
}

/// The label box a cell's own paint path measures its label against
/// (module-ui-label-fit D1): the drawn cell rect, which paint path applies, and
/// whether a state string exists for the label to yield to (D4).
///
/// `glyph_chars` is always `0`: the compact state glyph is a corner marker that
/// shares no line with the label (D3), so the label owns the cell's whole width
/// rather than paying for a glyph prefix it no longer sits beside.
pub(crate) fn cell_label_box(cell: &CellSpec) -> crate::physical::LabelBox {
    crate::physical::LabelBox {
        w: cell.rect.width(),
        h: cell.rect.height(),
        glyph_chars: 0,
        path: if cell.is_fader {
            crate::physical::CellTextPath::Fader
        } else if cell.shape.is_some() {
            crate::physical::CellTextPath::Composed
        } else {
            crate::physical::CellTextPath::Compact
        },
        has_state: !cell.state_text.is_empty(),
    }
}

/// The fitted label of one cell, computed once when the spec is built
/// (module-ui-label-fit D1). This is the single answer the painter, the
/// annotation filter, the hover reveal and the fit clamp read instead of
/// re-deriving a character budget per paint path.
///
/// `None` means the label is empty or no ladder step — not even the 6 pt floor
/// — can draw a character of it in this cell, so the cell shows no label and
/// the label presentation becomes its carrier (D9).
pub(crate) fn cell_label_layout(cell: &CellSpec) -> Option<crate::physical::LabelLayout> {
    crate::physical::label_layout(&cell.label, &cell_label_box(cell))
}

/// The placed annotations plus how many candidates the 30 % band budget had to
/// drop (module-ui-labels D10). The dropped count is what the pane title reports
/// as hidden labels.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LabelPlan {
    pub callouts: Vec<CalloutSpec>,
    pub dropped: usize,
}

/// Annotation drop family (D10's fixed priority): LEDs and jacks go first, then
/// buttons, then switches, then pots, and faders last. A fader is a knob or
/// encoder cell carrying the fader face, so `is_fader` outranks its kind.
fn annotation_family(cell: &CellSpec) -> u8 {
    if cell.is_fader {
        return 4;
    }
    match cell.kind {
        ComponentKind::Led | ComponentKind::CvIn | ComponentKind::CvOut => 0,
        ComponentKind::Button => 1,
        ComponentKind::Switch => 2,
        ComponentKind::Knob | ComponentKind::Encoder => 3,
    }
}

/// Band depth of the deepest placed label on each axis, measured from the rack
/// edge to the farthest label edge: vertical for the Over/Under bands, horizontal
/// for Left/Right. The 30 % budget bounds each axis against its own pane extent.
fn band_depths(rack: Rect, placed: &[crate::performance::PlacedLabel]) -> (f32, f32) {
    let mut depth_y = 0.0_f32;
    let mut depth_x = 0.0_f32;
    for label in placed {
        let r = label.label_rect;
        match label.side {
            crate::performance::Side::Over => {
                depth_y = depth_y.max(rack.min.y - r.y);
            }
            crate::performance::Side::Under => {
                depth_y = depth_y.max((r.y + r.h) - rack.max.y);
            }
            crate::performance::Side::Left => {
                depth_x = depth_x.max(rack.min.x - r.x);
            }
            crate::performance::Side::Right => {
                depth_x = depth_x.max((r.x + r.w) - rack.max.x);
            }
        }
    }
    (depth_y, depth_x)
}

/// Place the annotation candidates with `performance::place_labels` and enforce
/// the 30 % band budget (module-ui-labels D10): while the deepest band exceeds
/// 30 % of the pane on either axis, drop one candidate at a time in the fixed
/// family order. A `highlighted` cell — the focused element or the selected
/// circuit's hardware — is never dropped; if only protected candidates remain
/// the loop stops and keeps them, so the emphasis the user is following never
/// disappears.
pub(crate) fn budget_annotations(pane: Rect, rack_rect: Rect, cells: &[&CellSpec]) -> LabelPlan {
    if pane.width() <= 0.0 || pane.height() <= 0.0 {
        return LabelPlan::default();
    }
    let rack = crate::performance::Rect::new(
        rack_rect.min.x,
        rack_rect.min.y,
        rack_rect.width(),
        rack_rect.height(),
    );
    // Measure once: the drop loop only re-places.
    let measured: Vec<(f32, f32)> = cells
        .iter()
        .map(|cell| callout_size(&cell.label, &cell.state_text))
        .collect();
    // Drop order is total and deterministic: family first, candidate order to
    // break ties.
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by_key(|&i| (annotation_family(cells[i]), i));
    let place = |kept: &[bool]| {
        let inputs: Vec<crate::performance::LabelInput> = (0..cells.len())
            .filter(|&i| kept[i])
            .map(|i| crate::performance::LabelInput {
                host_id: i,
                host_rect: crate::performance::Rect::new(
                    cells[i].rect.min.x,
                    cells[i].rect.min.y,
                    cells[i].rect.width(),
                    cells[i].rect.height(),
                ),
                label_w: measured[i].0,
                label_h: measured[i].1,
            })
            .collect();
        crate::performance::place_labels(rack, &inputs)
    };
    let mut kept = vec![true; cells.len()];
    let mut placed = place(&kept);
    loop {
        let (depth_y, depth_x) = band_depths(rack_rect, &placed);
        if depth_y <= 0.3 * pane.height() && depth_x <= 0.3 * pane.width() {
            break;
        }
        match order
            .iter()
            .copied()
            .find(|&i| kept[i] && !cells[i].highlighted)
        {
            Some(next) => {
                kept[next] = false;
                placed = place(&kept);
            }
            // Every remaining candidate is protected: never drop one.
            None => break,
        }
    }
    let dropped = kept.iter().filter(|kept| !**kept).count();
    let mut callouts: Vec<CalloutSpec> = placed
        .into_iter()
        .map(|label| {
            let cell = cells[label.host_id];
            CalloutSpec {
                label_rect: Rect::from_min_size(
                    Pos2::new(label.label_rect.x, label.label_rect.y),
                    Vec2::new(label.label_rect.w, label.label_rect.h),
                ),
                anchor: Pos2::new(label.anchor.x, label.anchor.y),
                label: cell.label.clone(),
                state_text: cell.state_text.clone(),
                color: cell.color,
                highlighted: cell.highlighted,
            }
        })
        .collect();
    // Top-to-bottom paint order, mirroring `performance_overlay`.
    callouts.sort_by_key(|c| (c.label_rect.min.y * 4096.0) as i32);
    LabelPlan { callouts, dropped }
}

/// A copy of `cell` carrying the live state text when its paint path drops the
/// in-cell state row (a composed button face puts the state on its outline), so a
/// callout can show the state the cell itself cannot. Shared by the annotation
/// layer and the hover reveal.
fn with_resolved_state(cell: &CellSpec, patch: &crate::patch::Patch) -> CellSpec {
    let mut source = cell.clone();
    if source.state_text.is_empty() {
        if let Some(state) = cell
            .component_index
            .and_then(|i| patch.hw_components.get(i))
            .map(|comp| cell_visuals(comp, false, false, None).1)
        {
            source.state_text = state;
        }
    }
    source
}

/// The hover reveal (D11): the full label plus state of the hovered cell, as a
/// callout anchored to its edge. The side is chosen to stay inside the pane —
/// right of the cell when it fits, else left — and the rect is centred on the
/// cell vertically and clamped into the pane, so the callout never covers the
/// element it describes. `None` for an unlabelled or unaddressed cell.
fn hover_callout(pane: Rect, cell: &CellSpec) -> Option<CalloutSpec> {
    if cell.label.is_empty() || cell.component_index.is_none() {
        return None;
    }
    let (w, h) = callout_size(&cell.label, &cell.state_text);
    let gap = crate::performance::SIDE_MARGIN;
    let right = cell.rect.max.x + gap;
    let left = cell.rect.min.x - gap - w;
    // A side only counts when the whole callout lands inside the pane: a cell
    // outside or wider than the pane has no valid side at all.
    let fits_right = right >= pane.min.x && right + w <= pane.max.x;
    let fits_left = left >= pane.min.x && left + w <= pane.max.x;
    let (x, anchor_x) = if fits_right {
        (right, cell.rect.max.x)
    } else if fits_left {
        // Flip to the side that keeps the callout inside the pane.
        (left, cell.rect.min.x)
    } else {
        // Neither side has room (a cell outside or wider than the pane): clamp
        // into the pane, keeping the leader on the edge facing the callout.
        let clamped = (pane.max.x - w).max(pane.min.x);
        let anchor = if clamped < cell.rect.min.x {
            cell.rect.min.x
        } else {
            cell.rect.max.x
        };
        (clamped, anchor)
    };
    let y = (cell.rect.center().y - h / 2.0).clamp(pane.min.y, (pane.max.y - h).max(pane.min.y));
    Some(CalloutSpec {
        label_rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)),
        anchor: Pos2::new(anchor_x, cell.rect.center().y),
        label: cell.label.clone(),
        state_text: cell.state_text.clone(),
        color: cell.label_color,
        highlighted: cell.highlighted,
    })
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
        // A composed button face drops the in-cell state-text row (the shape
        // carries the state), so the callout resolves the live state text
        // from the component instead of going blank.
        let state_text = if cell.state_text.is_empty() {
            // Label helper is detached from shift context (state text only,
            // colour-independent); keep `false` per D1.
            // Label helper has no geometry cell in scope (detached text
            // context), so no kind to resolve — pass `None` (toggle set).
            cell_visuals(comp, false, false, None).1
        } else {
            cell.state_text.clone()
        };
        let (w, h) = callout_size(&headline, &state_text);
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
        meta.push((headline, state_text, cell.label_color, cell.highlighted));
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
    // Label annotations (module-ui-labels D10): the same side-bound callouts the
    // performance view draws, holding the labels the cells cannot show. Like the
    // performance overlay and the DB8E bands they are decoration — no hit rect.
    paint_callouts(&painter, &spec.annotations, spec.paused);
    // Hover reveal (D11): one callout for the element under the pointer.
    if let Some(hover) = &spec.hover {
        paint_callouts(&painter, std::slice::from_ref(hover), spec.paused);
    }

    drop(painter);
    ui.ctx().input(|i| physical_frame(i, pane, Some(spec)))
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

    // The composed face's outline is the state channel; every other path's
    // `base` is already the element's own colour. Labels never read this — they
    // use `cell.label_color`, which an off face keeps at the kind token.
    let base = cell.shift_color.unwrap_or(cell.color);

    if cell.is_fader {
        paint_fader(painter, cell, paused);
        return;
    }

    // Composed button face (button-led-composition D5/D6/D7): the outline
    // carries the button state, the folded LED is the inner fill, the label
    // renders under the shape when the cell has room, and the state-text row
    // is dropped. The fill stays bright while paused (fader-led-contrast);
    // only the outline and the label dim.
    if let Some(shape) = cell.shape {
        paint_composed(painter, cell, shape, base, paused);
        return;
    }

    // Standalone LED chrome for non-composed cells: outline ring plus inset
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

    // Compact cell (module-ui-label-fit D3/D4): the state glyph is a small
    // corner marker so it no longer shares a line with the label, then the
    // fitted label lines, then the state row the layout kept.
    let color = dim(rgb(cell.label_color), paused || cell.dimmed);
    paint_state_marker(painter, cell, color);
    let Some(layout) = cell.label_layout.as_ref() else {
        return;
    };
    let line_h = crate::physical::label_line_height(layout.font);
    let font = egui::FontId::monospace(layout.font);
    for (i, line) in layout.lines.iter().enumerate() {
        painter.text(
            rect.min + egui::vec2(LABEL_INSET_X, i as f32 * line_h),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            color,
        );
    }
    if layout.state_row {
        paint_state_row(painter, cell, layout, line_h, paused);
    }
}

/// The compact cell's state glyph as a corner marker (module-ui-label-fit D3):
/// `h × 0.28` points, never above 9 pt and never below the 6 pt legibility
/// floor — a cell too small for 6 pt draws no marker at all. The marker yields
/// to a cell's own LED indicator, which is then the state channel.
fn paint_state_marker(painter: &Painter, cell: &CellSpec, color: egui::Color32) {
    if cell.glyph.is_empty() || cell.led_state.is_some() {
        return;
    }
    let raw = cell.rect.height() * STATE_MARKER_SCALE;
    if raw < STATE_MARKER_MIN {
        return;
    }
    painter.text(
        Pos2::new(cell.rect.max.x - LABEL_INSET_X, cell.rect.min.y + 1.0),
        egui::Align2::RIGHT_TOP,
        &cell.glyph,
        egui::FontId::monospace(raw.clamp(STATE_MARKER_MIN, STATE_MARKER_MAX)),
        color,
    );
}

/// The in-cell state row (module-ui-label-fit D4): proportional
/// [`crate::physical::state_font_size`], one line height below the last label
/// line, and only when the layout kept room for it.
fn paint_state_row(
    painter: &Painter,
    cell: &CellSpec,
    layout: &crate::physical::LabelLayout,
    line_h: f32,
    paused: bool,
) {
    let font = egui::FontId::proportional(crate::physical::state_font_size(layout.font));
    let budget = (cell.rect.width() / (font.size * MONO_ADVANCE)).floor() as usize;
    painter.text(
        cell.rect.min + egui::vec2(LABEL_INSET_X, layout.lines.len() as f32 * line_h),
        egui::Align2::LEFT_TOP,
        clip_label(&cell.state_text, budget.max(1)),
        font,
        dim(rgb(crate::theme::active().muted), paused),
    );
}

/// Draw the performance overlay: one leader line plus a boxed big-label
/// callout per placed element. Headlines paint at [`CALLOUT_LABEL_SIZE`]
/// monospace (the size [`callout_size`] budgets); the state line paints
/// smaller and muted like the in-cell state text.
///
/// Draw a composed button face (button-led-composition D5/D6/D7): a state
/// outline (off `muted`, on the button token with a bolder stroke) whose
/// interior belongs to the folded LED as a concentric fill. The fill extent
/// is `0.25 floor + 0.75 × level` with the patch RGB scaled by level when
/// known, else white; an element with no bound LED renders the outline
/// alone. The shape keeps the cell at every preset and is never shrunk or
/// displaced for the label; the fitted label lines (module-ui-label-fit D6)
/// are bottom-aligned in the band beneath the shape, at most two of them, and
/// are omitted entirely — leaving the cell an annotation candidate — when the
/// layout found no step that fits the band. No state-text row: the shape carries
/// the state.
fn paint_composed(painter: &Painter, cell: &CellSpec, shape: CellShape, base: Color, paused: bool) {
    let rect = cell.rect;
    // The face spans the cell: the outline and fill stay concentric with the
    // cell center at every preset, and the label takes the band beneath it.
    let diameter = rect.width().min(rect.height()).max(1.0);
    let center = rect.center();
    // The outline is the state channel (D6): the build already resolved it
    // to `muted` (off) or the button token (on); a non-muted outline gets
    // the bolder stroke. Paused dims outline + label, never the fill.
    let on = base != crate::theme::active().muted;
    let stroke_w = if on { 2.0 } else { 1.0 };
    let ring = dim(rgb(base), paused);
    let fill_color = cell.led_state.map(|v| {
        let level = v.clamp(0.0, 1.0).max(0.25);
        if let Some(rgb) = cell.led_rgb {
            let scale = |c: u8| ((c as f32 * level).round().clamp(0.0, 255.0)) as u8;
            egui::Color32::from_rgb(scale(rgb[0]), scale(rgb[1]), scale(rgb[2]))
        } else {
            let intensity = (level * 255.0) as u8;
            egui::Color32::from_rgb(intensity, intensity, intensity)
        }
    });
    let extent = cell
        .led_state
        .map(|v| 0.25 + 0.75 * v.clamp(0.0, 1.0))
        .unwrap_or(0.0);
    match shape {
        CellShape::Round => {
            let radius = (diameter / 2.0 - 1.0).max(1.0);
            painter.circle_stroke(center, radius, egui::Stroke::new(stroke_w, ring));
            if let Some(fill) = fill_color {
                // The fill grows from the floor core to the full face: its
                // base is the full half-diameter (a hair past the ring, so
                // a lit face reads lit), never past half the cell's smaller
                // side.
                let fill_r = ((diameter / 2.0) * extent).max(1.0).min(diameter / 2.0);
                painter.circle_filled(center, fill_r, fill);
            }
        }
        CellShape::Rect => {
            let shape_rect =
                Rect::from_center_size(center, Vec2::new(diameter - 2.0, diameter - 2.0));
            painter.rect_stroke(
                shape_rect,
                0.0,
                egui::Stroke::new(stroke_w, ring),
                egui::StrokeKind::Inside,
            );
            if let Some(fill) = fill_color {
                // The touch plate fills as one rectangle: extent reads
                // through brightness (the floor keeps the field visible),
                // so the fill always spans the plate.
                let _ = extent;
                let fill_size = (diameter - 2.0).max(1.0);
                painter.rect_filled(
                    Rect::from_center_size(center, Vec2::new(fill_size, fill_size)),
                    0.0,
                    fill,
                );
            }
        }
    }
    // The label band (D6): the lines end at the cell's bottom edge, which is
    // how the face's lower arc leaves them room without the shape shrinking.
    if let Some(layout) = cell.label_layout.as_ref() {
        let line_h = crate::physical::label_line_height(layout.font);
        let font = egui::FontId::monospace(layout.font);
        let color = dim(rgb(cell.label_color), paused);
        let top = rect.max.y - layout.lines.len() as f32 * line_h;
        for (i, line) in layout.lines.iter().enumerate() {
            painter.text(
                Pos2::new(rect.min.x + LABEL_INSET_X, top + i as f32 * line_h),
                egui::Align2::LEFT_TOP,
                line,
                font.clone(),
                color,
            );
        }
    }
}
fn paint_performance(painter: &Painter, overlay: &PerformanceOverlay, paused: bool) {
    paint_callouts(painter, &overlay.callouts, paused);
}

/// Draw side-bound callouts: one leader line from the host-cell edge anchor to
/// the placed label rect, then the big monospace headline and the muted state
/// row. Shared by the performance overlay, the label annotations, and the hover
/// reveal, so every callout on the surface reads as one presentation.
fn paint_callouts(painter: &Painter, callouts: &[CalloutSpec], paused: bool) {
    let leader = rgb(crate::theme::active().performance_leader);
    let headline_font = egui::FontId::monospace(CALLOUT_LABEL_SIZE);
    let state_font = egui::FontId::proportional((CALLOUT_LABEL_SIZE * 0.8).max(9.0));
    for callout in callouts {
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
/// shade, and the fitted label + percentage in the column right of the track
/// (port of `ui.rs::render_fader_track`). The track narrows to 40 % of the cell
/// width, at most 10 pt (module-ui-label-fit D5), so a tall-narrow fader keeps a
/// real label column beside it; the label stacks one glyph per line when the
/// layout says the column is too narrow for a line of text.
fn paint_fader(painter: &Painter, cell: &CellSpec, paused: bool) {
    let rect = cell.rect;
    let track_w = (rect.width() * FADER_TRACK_WIDTH_FRACTION)
        .min(FADER_TRACK_WIDTH_MAX)
        .min(rect.width());
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

    // The label column beside the narrowed track (D5): horizontal lines when the
    // layout kept them, one glyph per line when it stacked them.
    let Some(layout) = cell.label_layout.as_ref() else {
        return;
    };
    let text_x = rect.min.x + track_w + FADER_TRACK_GUTTER;
    let line_h = crate::physical::label_line_height(layout.font);
    let font = egui::FontId::monospace(layout.font);
    let color = dim(rgb(cell.label_color), paused || cell.dimmed);
    for (i, line) in layout.lines.iter().enumerate() {
        painter.text(
            Pos2::new(text_x, rect.min.y + i as f32 * line_h),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            color,
        );
    }
    if layout.state_row {
        let state_font = egui::FontId::proportional(crate::physical::state_font_size(layout.font));
        let budget =
            (rect.width() - track_w - FADER_TRACK_GUTTER) / (state_font.size * MONO_ADVANCE);
        painter.text(
            Pos2::new(text_x, rect.min.y + layout.lines.len() as f32 * line_h),
            egui::Align2::LEFT_TOP,
            clip_label(&cell.state_text, (budget.floor() as usize).max(1)),
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
    switch_kind: Option<&str>,
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
            let round = switch_kind == Some("rotary");
            let (glyph, state) = match &comp.state {
                ComponentState::Value(v) => ("\u{25C9}", format!("{:.0}%", v * 100.0)),
                ComponentState::On if round => ("\u{25C9}", "ON".into()),
                ComponentState::On => ("\u{25A3}", "ON".into()),
                _ if round => ("\u{25CB}", "OFF".into()),
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
) -> RackGeometry {
    let cell = |mm: crate::physical::RectMm| -> Rect {
        let (x, y, w, h) = mapping.mm_to_screen(mm);
        Rect::from_min_size(Pos2::new(x as f32, y as f32), Vec2::new(w as f32, h as f32))
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
            // Unaddressed cells (unreferenced-elements D1/D3): every
            // geometry-declared cell no component addresses, in deterministic
            // family/row/col order, with `ci = components.len() + ordinal` so
            // the spec's `ci`-based branch renders them dimmed via the
            // existing `None` path. Same `cell()` mapping as addressed cells.
            // The auto-added master faceplate (`CV I/O`) is not a declared
            // controller, so its unused jacks/matrix stay unrendered — this
            // keeps bare full fronts byte-identical (D5).
            if module.controller != "CV I/O" {
                for (ordinal, unaddr) in module.unaddressed_cells().into_iter().enumerate() {
                    if unaddr.element.is_none() {
                        continue;
                    }
                    let mark = match unaddr.kind_letter {
                        Some('I') => PortMark::PortIn,
                        Some('O') => PortMark::PortOut,
                        _ => PortMark::Cell,
                    };
                    let rect = cell(crate::physical::RectMm {
                        x_mm: placed.rect_mm.x_mm + unaddr.rect_mm.x_mm,
                        y_mm: abs_y_mm + unaddr.rect_mm.y_mm,
                        w_mm: unaddr.rect_mm.w_mm,
                        h_mm: unaddr.rect_mm.h_mm,
                    });
                    cells.push((
                        placed.module_index,
                        module.components.len() + ordinal,
                        rect,
                        mark,
                    ));
                }
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
    w_mm: f64,
    h_mm: f64,
    step_mm: f64,
) -> Vec<(Pos2, Pos2)> {
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
            Pos2::new(px as f32, py as f32),
            Pos2::new(px as f32, (py + ph) as f32),
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
            Pos2::new(px as f32, py as f32),
            Pos2::new((px + pw) as f32, py as f32),
        ));
        y += step_mm;
    }
    lines
}

/// The frame's physical-view input from the egui input state: middle-drag
/// pan (points), the wheel's disambiguated intent, and a plain `s` skeleton
/// toggle. `pane` maps the absolute pointer (wheel zoom anchor, drag origin)
/// into the pane-local space the spec's points live in, and gates every
/// gesture to this pane so input over another surface can never move the rack.
/// `spec` resolves the wheel's D7 disambiguation: the hit cell decides whether
/// the wheel is a value adjust or a zoom. Pure so input tests run without a
/// window.
pub(super) fn physical_frame(
    i: &egui::InputState,
    pane: egui::Rect,
    spec: Option<&PhysicalSpec>,
) -> PhysicalFrame {
    let mut frame = PhysicalFrame::default();
    // Only a middle-drag whose press started inside this pane is ours; the
    // origin check keeps a drag begun on another pane from moving the rack.
    if i.pointer.middle_down()
        && i.pointer
            .press_origin()
            .is_some_and(|origin| pane.contains(origin))
    {
        let d = i.pointer.delta();
        frame.pan_delta = (d.x, d.y);
    }
    let pointer = i.pointer.latest_pos().filter(|p| pane.contains(*p));
    if let Some(p) = pointer {
        if let Some(factor) = wheel_zoom_factor(i) {
            let local = p - pane.min;
            // D7: `Ctrl` always zooms; otherwise the hit cell decides — an
            // adjustable element takes the value adjust, anything else in the
            // pane zooms about the pointer.
            if i.modifiers.ctrl {
                frame.zoom = Some((factor, (local.x, local.y)));
            } else if let Some(index) = adjustable_cell_at(spec, p) {
                frame.value_adjust = Some((index, wheel_value_step(i.smooth_scroll_delta.y)));
            } else {
                frame.zoom = Some((factor, (local.x, local.y)));
            }
        }
    }
    if i.key_pressed(egui::Key::S)
        && !(i.modifiers.shift || i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command)
    {
        frame.skeleton_toggle = true;
    }
    frame
}

/// This frame's wheel zoom factor at the graph canvas's sensitivity, clamped
/// to `MAX_ZOOM_STEP` (module-ui-labels D7). `Ctrl`+wheel is consumed by egui
/// as its own zoom (`zoom_factor_delta`) rather than the smooth scroll delta,
/// so that path is converted from egui's 1/200 scroll-zoom speed to the
/// canvas's 0.01 (exponent ratio 2) and inverted: egui's factor grows on the
/// same scroll sign this app treats as zoom-out, so wheel-up must zoom in.
/// `None` without a wheel event.
fn wheel_zoom_factor(i: &egui::InputState) -> Option<f32> {
    let scroll = i.smooth_scroll_delta.y;
    let factor = if scroll.abs() > f32::EPSILON {
        ((-scroll) * ZOOM_SENSITIVITY).exp()
    } else if i.modifiers.ctrl {
        let egui_zoom = i.zoom_delta();
        if (egui_zoom - 1.0).abs() <= f32::EPSILON {
            return None;
        }
        egui_zoom.powf(-2.0)
    } else {
        return None;
    };
    Some(factor.clamp(1.0 / MAX_ZOOM_STEP, MAX_ZOOM_STEP))
}

/// The `patch.hw_components` index of the adjustable element under `pos`
/// (module-ui-labels D7): a knob, an encoder, or a fader face. Buttons,
/// switches, LEDs, and CV jacks are not adjustable, so the wheel over them is
/// a zoom.
fn adjustable_cell_at(spec: Option<&PhysicalSpec>, pos: Pos2) -> Option<usize> {
    spec?.cells.iter().find_map(|cell| {
        if !cell.rect.contains(pos) {
            return None;
        }
        if !(cell.is_fader
            || matches!(
                cell.kind,
                crate::patch::ComponentKind::Knob | crate::patch::ComponentKind::Encoder
            ))
        {
            return None;
        }
        cell.component_index
    })
}

/// One wheel notch's value step: up (egui reports negative Y scrolling up)
/// raises the value, mirroring the terminal `ScrollUp` path.
fn wheel_value_step(smooth_scroll_delta_y: f32) -> f32 {
    if smooth_scroll_delta_y < 0.0 {
        WHEEL_VALUE_STEP
    } else {
        -WHEEL_VALUE_STEP
    }
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
    use crate::config::LabelPresentation;
    use crate::patch::{ComponentState, HwComponent};
    use crate::physical::{ChainGaps, PlacedModule, RackMounts, RackRowPlacement};

    /// The fixed cell-label size the surface drew before per-cell fitting
    /// (label-guess-and-screen-scale, task 2.2). Kept here as the comparison
    /// point the paint tests measure a fitted step against: the ladder's top
    /// step is 10 pt, and every test asserts a cell never pays more than it did
    /// under the fixed font.
    const CELL_LABEL_SIZE: f32 = 10.0;

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
                            kind: None,
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
                                kind: None,
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
                                kind: None,
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
        ScreenMapping::new(crate::physical::PHYSICAL_POINTS_PER_MM, zoom, 0.0, 0.0)
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

    /// A hand-built `CellSpec` with its label layout resolved exactly as
    /// `physical_spec` resolves it, so headless paint tests draw the fitted
    /// label the production build would draw (module-ui-label-fit D1).
    fn resolved_cell(mut cell: CellSpec) -> CellSpec {
        cell.label_layout = cell_label_layout(&cell);
        cell
    }

    fn build_spec(
        fx: (&RackLayout, &PhysicalLayout),
        skeleton: bool,
        paused: bool,
        zoom: f64,
    ) -> PhysicalSpec {
        let (rack, chain) = fx;
        let m = mapping_at(zoom);
        let geom = rack_geometry(rack, chain, &m);
        let mut cells = Vec::new();
        for &(mi, ci, rect, mark) in &geom.cells {
            let comp = &chain.modules[mi].components[ci];
            // Test fixture has no geometry loaded; kind is unresolvable here.
            let (glyph, state_text, color) = cell_visuals(comp, false, false, None);
            cells.push(resolved_cell(CellSpec {
                rect,
                glyph,
                label: comp.label.clone(),
                state_text,
                color,
                label_color: color,
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
                shape: None,
                label_layout: None,
            }));
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
            grid_lines: mm_grid_lines(&m, rack.total_width_mm, rack.total_height_mm, 20.0),
            performance: None,
            annotations: Vec::new(),
            hover: None,
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
            crate::physical::PHYSICAL_POINTS_PER_MM,
            app.physical_zoom as f64,
            app.physical_offset.0 as f64,
            app.physical_offset.1 as f64,
        );
        let geom = rack_geometry(&rack, &chain, &m);
        // A zero-origin pane keeps the spec rack-local so it matches `geom`.
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("a loaded patch yields a spec");
        // Folded LEDs publish no cell (button-led-composition D4), so the
        // spec holds the geometry cells minus the folded LED cells.
        let mut folded: std::collections::HashSet<String> = std::collections::HashSet::new();
        for module in chain.modules.iter() {
            for comp in &module.components {
                if comp.kind != crate::patch::ComponentKind::Button {
                    continue;
                }
                if fold_shape_for_controller(&comp.controller).is_none() {
                    continue;
                }
                if let Some(led) = crate::patch::folded_led_for_element(comp) {
                    folded.insert(led);
                }
            }
        }
        let expected: Vec<(Option<usize>, egui::Rect)> = geom
            .cells
            .iter()
            .filter(|(mi, ci, _, _)| {
                *ci >= chain.modules[*mi].components.len()
                    || !folded.contains(&chain.modules[*mi].components[*ci].id)
            })
            .map(|(mi, ci, rect, _)| {
                let global = if *ci < chain.modules[*mi].components.len() {
                    let comp = &chain.modules[*mi].components[*ci];
                    patch
                        .hw_components
                        .iter()
                        .position(|c| c.id == comp.id)
                        .expect("every chain component exists in the patch")
                } else {
                    // Unaddressed geometry cell: the spec renders it dimmed
                    // with `component_index: None` (never hit-testable).
                    return (None, *rect);
                };
                (Some(global), *rect)
            })
            .collect();
        assert_eq!(spec.cells.len(), expected.len());
        for (cell, (global, rect)) in spec.cells.iter().zip(expected.iter()) {
            assert_eq!(cell.component_index, *global);
            assert_eq!(cell.rect, *rect);
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
        // Published indices are distinct and in range; unused geometry cells
        // carry `None` and never publish a hit rect (mirror of the
        // gui/mod.rs filter).
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
        let patch = app.patch.as_ref().expect("patch");
        let mut checked = 0;
        for i in indices {
            // A folded LED publishes no cell (button-led-composition D4);
            // its element's cell carries the highlight instead.
            if patch.hw_components.get(i).is_some_and(|c| {
                c.kind == crate::patch::ComponentKind::Led
                    && spec
                        .cells
                        .iter()
                        .all(|cell| cell.component_index != Some(i))
            }) {
                continue;
            }
            let cell = spec
                .cells
                .iter()
                .find(|c| c.component_index == Some(i))
                .unwrap_or_else(|| panic!("cell for hardware index {i}"));
            assert!(cell.highlighted, "circuit cell {i} is highlighted");
            checked += 1;
        }
        assert!(checked > 0, "at least one circuit cell stays highlighted");
    }

    #[test]
    fn rack_geometry_maps_mm_to_points() {
        let (rack, chain) = fixture();
        let geom = rack_geometry(&rack, &chain, &mapping());
        // 200×40 mm at the true-1:1 3.7795 pt/mm baseline (D1).
        let ppm = crate::physical::PHYSICAL_POINTS_PER_MM as f32;
        assert_eq!(
            geom.case_rect,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0 * ppm, 40.0 * ppm))
        );
        assert_eq!(geom.module_rects.len(), 1);
        assert_eq!(geom.module_rects[0].0, 0);
        assert_eq!(
            geom.module_rects[0].1,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0 * ppm, 40.0 * ppm))
        );
        // The B1.1 cell: 20×10 mm at (10,5) mm → true size in points.
        assert_eq!(geom.cells.len(), 1);
        let (_, _, rect, mark) = geom.cells[0];
        assert_eq!(mark, PortMark::Cell);
        assert!((rect.min.x - 10.0 * ppm).abs() < 1e-3);
        assert!((rect.min.y - 5.0 * ppm).abs() < 1e-3);
        assert!((rect.width() - 20.0 * ppm).abs() < 1e-3);
        assert!((rect.height() - 10.0 * ppm).abs() < 1e-3);
    }

    #[test]
    fn mm_grid_lines_span_the_rack_at_step_intervals() {
        let lines = mm_grid_lines(&mapping(), 200.0, 40.0, 20.0);
        // 20 mm steps: verticals every 75.59 pt across 200 mm, one horizontal.
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
        let (g, s, c) = cell_visuals(&on, false, false, None);
        assert_eq!(g, "\u{25CF}");
        assert_eq!(s, "ON");
        assert_eq!(c, t.button);
        // Shift-active button takes the group token.
        let shifted = HwComponent {
            shift_group: Some(ShiftGroup::Group2),
            ..on.clone()
        };
        let (_, _, c) = cell_visuals(&shifted, true, false, None);
        assert_eq!(c, t.shift2);
        // Fader-marked knob renders the LED strip state.
        let knob = HwComponent {
            kind: ComponentKind::Knob,
            state: ComponentState::Value(0.5),
            ..on
        };
        let (g, s, c) = cell_visuals(&knob, false, true, None);
        assert_eq!(g, "\u{25AE}");
        assert_eq!(s, "50%");
        assert_eq!(c, t.fader_led_bar);
    }

    fn switch_glyph_for_kind(kind: Option<&str>, comp: &HwComponent) -> (String, String) {
        let (g, s, _) = cell_visuals(comp, false, false, kind);
        (g, s)
    }

    fn switch_comp(state: ComponentState) -> HwComponent {
        HwComponent {
            id: "S1.1".into(),
            label: "S1.1".into(),
            kind: ComponentKind::Switch,
            shift_group: None,
            state,
            controller: "S10".into(),
            led: None,
            led_capability: None,
        }
    }

    #[test]
    fn switch_rotary_kind_uses_round_glyphs() {
        let (g, _) = switch_glyph_for_kind(Some("rotary"), &switch_comp(ComponentState::On));
        assert_eq!(g, "\u{25C9}");
        let (g, _) = switch_glyph_for_kind(Some("rotary"), &switch_comp(ComponentState::Off));
        assert_eq!(g, "\u{25CB}");
        let (g, s) =
            switch_glyph_for_kind(Some("rotary"), &switch_comp(ComponentState::Value(0.5)));
        assert_eq!(g, "\u{25C9}");
        assert_eq!(s, "50%");
    }

    #[test]
    fn switch_toggle_and_unannotated_kinds_use_square_glyphs() {
        for kind in [Some("small"), Some("touch"), None] {
            let (g, _) = switch_glyph_for_kind(kind, &switch_comp(ComponentState::On));
            assert_eq!(g, "\u{25A3}");
            let (g, _) = switch_glyph_for_kind(kind, &switch_comp(ComponentState::Off));
            assert_eq!(g, "\u{25A1}");
        }
    }

    #[test]
    fn switch_state_text_unchanged_across_kinds() {
        for kind in [Some("rotary"), Some("small"), None] {
            let (_, s) = switch_glyph_for_kind(kind, &switch_comp(ComponentState::On));
            assert_eq!(s, "ON");
            let (_, s) = switch_glyph_for_kind(kind, &switch_comp(ComponentState::Off));
            assert_eq!(s, "OFF");
            let (_, s) = switch_glyph_for_kind(kind, &switch_comp(ComponentState::Value(0.5)));
            assert_eq!(s, "50%");
        }
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
        resolved_cell(CellSpec {
            rect: Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(40.0, 100.0)),
            glyph: String::new(),
            label: "F".to_string(),
            state_text: "50%".to_string(),
            color: crate::theme::active().knob,
            label_color: crate::theme::active().knob,
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
            shape: None,
            label_layout: None,
        })
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

    /// A compact full-presentation button cell for the label tests: no
    /// interaction flags, its label layout resolved as the build resolves it.
    fn cell_spec(rect: Rect, label: &str) -> CellSpec {
        resolved_cell(CellSpec {
            rect,
            glyph: "\u{25CF}".to_string(),
            label: label.to_string(),
            state_text: "ON".to_string(),
            color: crate::theme::active().button,
            label_color: crate::theme::active().button,
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
            shape: None,
            label_layout: None,
        })
    }

    #[test]
    fn paint_draws_module_title_and_cell_labels() {
        // Isotropic cells are half as tall as under the old 2:1 stretch, so
        // the 15 pt fixture cell no longer fits the state-text second row
        // (D1); at the 200 % preset it does, and the state path is unchanged.
        let labels = painted_labels(&spec_at_zoom(2.0));
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

    /// module-ui-label-fit D2 (overturning label-guess-and-screen-scale's fixed
    /// font): the step is chosen from each cell's own rendered size at that
    /// zoom — a cell that grows may take a larger step — every label run is a
    /// ladder step, and no run (the corner state marker included) drops below
    /// the 6 pt floor or exceeds the old fixed font.
    #[test]
    fn cell_label_step_follows_the_cell_size_across_zoom_presets() {
        let mut widest = Vec::new();
        for &zoom in &[0.75f64, 1.0, 1.5, 2.0] {
            let spec = spec_at_zoom(zoom);
            let rows: Vec<(String, egui::FontId, f32)> = painted_texts(&spec)
                .into_iter()
                .filter(|(_, font, _)| font.family == egui::FontFamily::Monospace)
                .collect();
            assert!(!rows.is_empty(), "cell label rows drawn at zoom {zoom}");
            // A label run carries the element's name; the state glyph is the
            // single-character corner marker (module-ui-label-fit D3) and is not
            // a ladder step.
            let label_runs: Vec<&(String, egui::FontId, f32)> = rows
                .iter()
                .filter(|(text, _, _)| text.chars().any(|c| c.is_alphanumeric()))
                .collect();
            assert!(
                label_runs.iter().any(|(text, _, _)| text.contains("B1.1")),
                "the cell keeps its label at zoom {zoom}: {rows:?}"
            );
            for (text, font, _) in &rows {
                assert!(
                    font.size >= LABEL_FLOOR,
                    "no run below the floor at zoom {zoom}: {text:?} at {} pt",
                    font.size
                );
                assert!(
                    font.size <= CELL_LABEL_SIZE,
                    "nothing exceeds the old fixed font at zoom {zoom}: {text:?}"
                );
            }
            for (text, font, _) in &label_runs {
                assert_ladder_step(font.size, &format!("zoom {zoom} {text:?}"));
            }
            widest.push(
                label_runs
                    .iter()
                    .map(|(_, font, _)| font.size)
                    .fold(0.0f32, f32::max),
            );
        }
        assert!(
            widest[3] >= widest[0],
            "a cell that grows may take a larger step: {widest:?}"
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

    /// module-ui-label-fit D2/D3 (overturning "ellipsize at the fixed font"): a
    /// narrow-but-wide-enough cell shows its whole label wrapped onto fitted
    /// lines, and no painted line ever overflows the cell width.
    #[test]
    fn cell_label_lines_stay_inside_the_cell_width() {
        let char_w = MONO_ADVANCE * CELL_LABEL_SIZE;
        let rect = Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::new(char_w * 8.0, 24.0));
        let texts = painted_cell(&cell_spec(rect, "averylonglabel"));
        let rows: Vec<&(String, egui::FontId, f32)> = texts
            .iter()
            .filter(|(_, font, _)| font.family == egui::FontFamily::Monospace)
            .collect();
        assert!(!rows.is_empty(), "the label is drawn: {texts:?}");
        assert!(
            !rows.iter().any(|(row, _, _)| row.contains('\u{2026}')),
            "the whole label fits when wrapped, so no floor ellipsis: {texts:?}"
        );
        for (row, _, width) in &rows {
            assert!(
                *width <= rect.width() + 1.0,
                "row fits the cell width: {width} pt > {} pt ({row:?})",
                rect.width()
            );
        }
    }

    #[test]
    fn rack_geometry_derives_db8e_band_from_b_grid_top() {
        let (rack, chain) = db8e_fixture();
        let geom = rack_geometry(&rack, &chain, &mapping());
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
        // The band bottom sits at/above the B-grid top (y_mm 38 → true size).
        let b_top_pt = 38.0 * crate::physical::PHYSICAL_POINTS_PER_MM as f32;
        assert!(band.max.y <= b_top_pt + 1e-3, "y {}", band.max.y);
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
            frame = ui.ctx().input(|i| physical_frame(i, ui.max_rect(), None));
        });
        full_output.textures_delta.clear();
        // 20px drag in points: the mapping yields points directly.
        assert_eq!(frame.pan_delta, (20.0, 10.0));
        // Wheel up 50px → zoom-in factor > 1 anchored at the cursor. Without a
        // spec there is no adjustable cell to hit, so the wheel is a zoom (D7).
        assert_eq!(frame.value_adjust, None);
        let (factor, anchor) = frame.zoom.expect("wheel zoom reported");
        assert!(factor > 1.0, "wheel-up zooms in: {factor}");
        assert_eq!(anchor, (120.0, 110.0));
        assert!(frame.skeleton_toggle, "plain `s` toggles the skeleton");
    }

    // -- module-ui-labels 1.6: the frame report in production ---------------

    /// One egui frame carrying `events`, after a parking frame that puts the
    /// pointer at `park` (drag deltas need a previous position; a wheel needs a
    /// pointer). Returns the physical input report the paint routine would.
    fn frame_after(
        park: Pos2,
        events: Vec<egui::Event>,
        pane: Rect,
        spec: Option<&PhysicalSpec>,
        modifiers: egui::Modifiers,
    ) -> PhysicalFrame {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let parked = egui::RawInput {
            screen_rect: Some(screen),
            events: vec![egui::Event::PointerMoved(park)],
            ..Default::default()
        };
        let mut out = ctx.run_ui(parked, |_| {});
        out.textures_delta.clear();
        // egui 0.36 takes modifier state from `ModifiersChanged` events, not a
        // `RawInput` field (the winit bridge sends one when the user holds a
        // key), so synthesize it ahead of the wheel/key events.
        let mut events = events;
        if modifiers != egui::Modifiers::default() {
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
        }
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        let mut frame = PhysicalFrame::default();
        let mut out = ctx.run_ui(raw, |ui| {
            frame = ui.ctx().input(|i| physical_frame(i, pane, spec));
        });
        out.textures_delta.clear();
        frame
    }

    fn wheel(up: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, if up { -50.0 } else { 50.0 }),
            modifiers,
            phase: egui::TouchPhase::Move,
        }
    }

    #[test]
    fn wheel_over_a_fader_adjusts_only_its_value() {
        // D7: the hit cell decides. A fader (a knob-kind cell with the fader
        // face) takes the value step; the camera and the selection stay put.
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut spec = spec(false, false);
        let mut cell = spec.cells[0].clone();
        cell.rect = Rect::from_min_size(Pos2::new(100.0, 400.0), Vec2::new(40.0, 120.0));
        cell.kind = crate::patch::ComponentKind::Knob;
        cell.is_fader = true;
        cell.component_index = Some(0);
        spec.cells = vec![cell];

        let mut app = App::new();
        app.patch = Some(
            crate::patch::Patch::from_ini_str(
                "[pot]\n    pot = P1.1\n    output = _X\n",
                String::from("wheel"),
            )
            .unwrap(),
        );
        let frame = frame_after(
            Pos2::new(120.0, 460.0),
            vec![wheel(true, egui::Modifiers::default())],
            pane,
            Some(&spec),
            egui::Modifiers::default(),
        );
        assert_eq!(frame.value_adjust, Some((0, WHEEL_VALUE_STEP)));
        assert_eq!(frame.zoom, None, "a value wheel must not zoom");

        let zoom_before = app.physical_zoom;
        let offset_before = app.physical_offset;
        crate::handler::handle_physical_frame(frame, &mut app);
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => {
                assert!((v - WHEEL_VALUE_STEP).abs() < 1e-6, "value: {v}")
            }
            _ => panic!("expected Value state"),
        }
        assert_eq!(app.physical_zoom, zoom_before, "no zoom on a value wheel");
        assert_eq!(app.physical_offset, offset_before);
        assert!(
            !app.physical_nav_latched,
            "a value adjust is not navigation"
        );
        assert!(app.selected_component.is_none() && app.hovered_component.is_none());
    }

    #[test]
    fn ctrl_wheel_over_a_fader_zooms_anywhere() {
        // D7: `Ctrl` is the unambiguous escape hatch over an adjustable cell.
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut spec = spec(false, false);
        let mut cell = spec.cells[0].clone();
        cell.rect = Rect::from_min_size(Pos2::new(100.0, 400.0), Vec2::new(40.0, 120.0));
        cell.kind = crate::patch::ComponentKind::Knob;
        cell.is_fader = true;
        cell.component_index = Some(0);
        spec.cells = vec![cell];

        let ctrl = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let frame = frame_after(
            Pos2::new(120.0, 460.0),
            vec![wheel(true, ctrl)],
            pane,
            Some(&spec),
            ctrl,
        );
        assert_eq!(frame.value_adjust, None, "Ctrl always zooms");
        let (factor, _) = frame.zoom.expect("Ctrl+wheel zooms over the fader");
        assert!(factor > 1.0, "wheel-up still zooms in: {factor}");
    }

    #[test]
    fn wheel_off_a_cell_zooms_about_the_pointer_only() {
        // D7: a wheel inside the pane but off every adjustable element zooms
        // about the pointer; the cursor-anchored offset is the frame's anchor.
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let spec = spec(false, false); // cells hug the rack near the top
        let pointer = Pos2::new(700.0, 550.0);
        assert!(!spec.cells.iter().any(|c| c.rect.contains(pointer)));

        let frame = frame_after(
            pointer,
            vec![wheel(true, egui::Modifiers::default())],
            pane,
            Some(&spec),
            egui::Modifiers::default(),
        );
        assert_eq!(frame.value_adjust, None);
        let (factor, anchor) = frame.zoom.expect("wheel off a cell zooms");
        assert!(factor > 1.0, "wheel-up zooms in: {factor}");
        assert_eq!(anchor, (700.0, 550.0), "pane-local cursor anchor");

        let mut app = App::new();
        crate::handler::handle_physical_frame(frame, &mut app);
        let ratio = app.physical_zoom;
        assert!(ratio > 1.0, "wheel-up zooms in: {ratio}");
        // `offset = offset·ratio + anchor·(ratio − 1)`, so the content under
        // the cursor keeps its screen position while the view scales.
        assert!((app.physical_offset.0 - 700.0 * (ratio - 1.0)).abs() < 1e-3);
        assert!((app.physical_offset.1 - 550.0 * (ratio - 1.0)).abs() < 1e-3);
        assert!(
            app.physical_nav_latched,
            "a wheel zoom latches the fit (D4)"
        );
    }

    #[test]
    fn middle_drag_moves_the_offset_and_latches() {
        // D8: a middle-drag inside the pane arrives as a pan delta and moves
        // the camera without touching zoom, selection, or focus.
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let events = vec![
            egui::Event::PointerButton {
                pos: Pos2::new(200.0, 200.0),
                button: egui::PointerButton::Middle,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerMoved(Pos2::new(230.0, 180.0)),
        ];
        let frame = frame_after(
            Pos2::new(200.0, 200.0),
            events,
            pane,
            None,
            egui::Modifiers::default(),
        );
        assert_eq!(frame.pan_delta, (30.0, -20.0));

        let mut app = App::new();
        let zoom_before = app.physical_zoom;
        crate::handler::handle_physical_frame(frame, &mut app);
        assert_eq!(app.physical_offset, (30.0, -20.0));
        assert_eq!(app.physical_zoom, zoom_before, "pan must not zoom");
        assert!(
            app.physical_nav_latched,
            "a manual pan latches the fit (D4)"
        );
        assert!(app.selected_component.is_none() && app.hovered_component.is_none());
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
        assert!(after.contains("ON"), "On state shows: {after}");
    }

    fn painted_shapes(cell: &CellSpec, paused: bool) -> Vec<egui::epaint::Shape> {
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
        let shapes: Vec<egui::epaint::Shape> = full_output
            .shapes
            .iter()
            .map(|cs| cs.shape.clone())
            .collect();
        full_output.textures_delta.clear();
        shapes
    }

    #[test]
    fn composed_p2b8_button_face_is_state_outline_with_concentric_disc() {
        let app = app_with_patch("fixtures/own_buttons.ini");
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let idx = patch
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .expect("fixture declares B1.1");
        let cell = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .expect("B1.1 cell")
            .clone();
        assert!(
            cell.glyph.is_empty(),
            "composed face carries no glyph: {:?}",
            cell.glyph
        );
        assert!(
            cell.state_text.is_empty(),
            "composed face carries no state-text row: {:?}",
            cell.state_text
        );
        let shapes = painted_shapes(&cell, false);
        let t = crate::theme::active();
        let on_stroke = rgb(t.button);
        let off_stroke = rgb(t.muted);
        let hits: Vec<(Pos2, f32)> = shapes
            .iter()
            .filter_map(|s| match s {
                egui::epaint::Shape::Circle(c) => Some((c.center, c.radius, c.stroke.color)),
                _ => None,
            })
            .filter(|(center, radius, color)| {
                let expected = cell.rect.center();
                (center.x - expected.x).abs() < 0.5
                    && (center.y - expected.y).abs() < 0.5
                    && *radius * 2.0 >= 0.8 * cell.rect.width().min(cell.rect.height())
                    && (*color == on_stroke || *color == off_stroke)
            })
            .map(|(center, radius, _)| (center, radius))
            .collect();
        assert!(
            !hits.is_empty(),
            "concentric state-outline circle: {shapes:?}"
        );
    }

    #[test]
    fn off_composed_face_keeps_its_label_bright_while_the_outline_carries_the_state() {
        // The composed outline is the state channel (`muted` when off). A label
        // must not inherit it: that painted every off button's label dark grey.
        let app = app_with_patch("fixtures/own_buttons.ini");
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let t = crate::theme::active();
        let cell = spec
            .cells
            .iter()
            .find(|c| c.shape.is_some() && c.component_index.is_some())
            .expect("a composed element cell");
        assert_eq!(
            cell.color, t.muted,
            "an off face's outline is the muted state token"
        );
        assert_eq!(
            cell.label_color, t.button,
            "the label keeps the element's own token, not the state outline"
        );
        assert_ne!(cell.label_color, t.muted, "labels never go dark grey");
    }

    #[test]
    fn composed_label_text_paints_in_the_label_colour() {
        let t = crate::theme::active();
        let mut cell = composed_cell(12.0, 12.0, "Trig", CellShape::Round);
        // An off face: muted outline, element token for the label.
        cell.color = t.muted;
        cell.label_color = t.button;
        let painted: Vec<(String, egui::Color32)> = painted_shapes(&cell, false)
            .iter()
            .filter_map(|s| match s {
                egui::epaint::Shape::Text(text) => {
                    Some((text.galley.text().to_string(), text.fallback_color))
                }
                _ => None,
            })
            .collect();
        assert!(
            !painted.is_empty(),
            "the composed band draws its fitted label"
        );
        for (text, color) in &painted {
            assert_eq!(
                *color,
                rgb(t.button),
                "label {text:?} paints in the label colour: {painted:?}"
            );
        }
    }

    #[test]
    fn a_shift_tints_the_label_as_well_as_the_face() {
        let t = crate::theme::active();
        let mut cell = composed_cell(12.0, 12.0, "Trig", CellShape::Round);
        cell.color = t.shift2;
        cell.shift_color = Some(t.shift2);
        cell.label_color = t.shift2;
        let colors: Vec<egui::Color32> = painted_shapes(&cell, false)
            .iter()
            .filter_map(|s| match s {
                egui::epaint::Shape::Text(text) => Some(text.fallback_color),
                _ => None,
            })
            .collect();
        assert!(!colors.is_empty(), "a label line is drawn");
        assert!(
            colors.iter().all(|c| *c == rgb(t.shift2)),
            "shift tint reaches the label: {colors:?}"
        );
    }

    #[test]
    fn callout_headline_takes_the_label_colour_not_the_state_outline() {
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        let t = crate::theme::active();
        let mut cell = pot_cell("Volume");
        cell.color = t.muted;
        cell.label_color = t.knob;
        let callout = hover_callout(pane, &cell).expect("callout for an addressed cell");
        assert_eq!(
            callout.color, t.knob,
            "the callout headline follows the label colour"
        );
        assert_ne!(
            callout.color, t.muted,
            "a callout never inherits a muted state outline"
        );
    }

    #[test]
    fn composed_led_fill_uses_patch_rgb_not_white() {
        // (led-register-colour D4): a button section carrying `ledcolor` /
        // `ledvalue` loads its folded LED as `Led { value, rgb }`, and the
        // composed face paints the fill in that rgb — not the white
        // fallback. Red until the loader resolver lands (task 2.1): the LED
        // still loads as `Off`, so `led_rgb` is `None` and the fill is white.
        let ini = r#"
[p2b8]

[button]
    button = B1.1
    led = L1.1
    ledcolor = 1.0
    ledvalue = 1.0
    output = _OWN_A

[button]
    button = B1.2
    led = L1.2
    output = _OWN_B
"#;
        let patch = crate::patch::Patch::from_ini_str(ini, "led-rgb".to_string())
            .expect("test patch parses");
        let mut app = App::new();
        assert!(app.load_patch(patch));
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let idx = patch
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .expect("patch declares B1.1");
        let cell = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .expect("B1.1 cell")
            .clone();
        let rgb = cell.led_rgb.expect("folded LED resolves a patch rgb");
        let expected = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        let white = egui::Color32::WHITE;
        assert!(expected != white, "resolved rgb is a real colour: {rgb:?}");
        let fills: Vec<egui::Color32> = painted_shapes(&cell, false)
            .iter()
            .filter_map(|s| match s {
                egui::epaint::Shape::Circle(c) => Some((c.center, c.radius, c.fill)),
                _ => None,
            })
            .filter(|(center, _, _)| {
                let want = cell.rect.center();
                (center.x - want.x).abs() < 0.5 && (center.y - want.y).abs() < 0.5
            })
            .map(|(_, _, fill)| fill)
            .collect();
        assert!(
            fills.contains(&expected),
            "inner fill paints the patch rgb, not white: {fills:?}"
        );
    }

    #[test]
    fn composed_led_fill_grows_with_level_and_stays_inside_ring() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(40.0, 40.0));
        let mut lo = cell_spec(rect, "B");
        lo.led_state = Some(0.0);
        lo.shape = Some(CellShape::Round);
        let mut hi = cell_spec(rect, "B");
        hi.led_state = Some(1.0);
        hi.shape = Some(CellShape::Round);
        let radius_of = |cell: &CellSpec| -> f32 {
            painted_shapes(cell, false)
                .iter()
                .filter_map(|s| match s {
                    egui::epaint::Shape::Circle(c) => Some((c.center, c.radius)),
                    _ => None,
                })
                .filter(|(center, _)| {
                    (center.x - rect.center().x).abs() < 0.5
                        && (center.y - rect.center().y).abs() < 0.5
                })
                .map(|(_, radius)| radius)
                .fold(0.0f32, f32::max)
        };
        let r_lo = radius_of(&lo);
        let r_hi = radius_of(&hi);
        assert!(r_lo > 0.0, "off LED still shows its floor disc");
        assert!(
            r_hi > r_lo,
            "full level fills further than the floor: {r_lo} vs {r_hi}"
        );
        assert!(
            r_hi <= rect.width().min(rect.height()) / 2.0 + 0.5,
            "inner disc never exceeds the ring: {r_hi}"
        );
    }

    #[test]
    fn composed_m4_touch_plate_is_rectangle_fill() {
        let app = app_with_patch("fixtures/device_led_defaults.ini");
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let button_idx = patch
            .hw_components
            .iter()
            .position(|c| c.kind == ComponentKind::Button && c.controller == "M4")
            .expect("M4 touch button");
        let cell = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(button_idx))
            .expect("M4 button cell")
            .clone();
        assert!(cell.glyph.is_empty(), "composed M4 face carries no glyph");
        assert!(
            cell.state_text.is_empty(),
            "composed M4 face carries no state row"
        );
        let rects = painted_cell_rects(&cell, false);
        let concentric_fill = rects.iter().any(|(r, fill, w, _)| {
            *w == 0.0
                && *fill != egui::Color32::TRANSPARENT
                && (r.center().x - cell.rect.center().x).abs() < 0.5
                && (r.center().y - cell.rect.center().y).abs() < 0.5
                && r.width() >= 0.8 * cell.rect.width().min(cell.rect.height())
        });
        assert!(concentric_fill, "concentric M4 rectangle fill: {rects:?}");
    }

    fn assert_no_led_cells(app: &App, ctx: &str) {
        let patch = app.patch.as_ref().expect("patch");
        let spec = physical_spec(app, egui::Rect::ZERO).expect("spec");
        let led_ids: Vec<usize> = patch
            .hw_components
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == ComponentKind::Led)
            .map(|(i, _)| i)
            .collect();
        assert!(!led_ids.is_empty(), "{ctx}: patch declares LEDs");
        for i in led_ids {
            let id = &patch.hw_components[i].id;
            assert!(
                !spec.cells.iter().any(|c| c.component_index == Some(i)),
                "{ctx}: folded LED {id} publishes no cell"
            );
        }
    }

    #[test]
    fn folded_leds_publish_no_cell_on_p2b8_b32_m4() {
        let app = app_with_patch("fixtures/own_buttons.ini");
        assert_no_led_cells(&app, "P2B8 own_buttons");
        let app = app_with_patch("fixtures/device_led_defaults.ini");
        {
            let patch = app.patch.as_ref().expect("patch");
            let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
            let led_ids: Vec<usize> = patch
                .hw_components
                .iter()
                .enumerate()
                .filter(|(_, c)| c.kind == ComponentKind::Led && c.controller == "M4")
                .map(|(i, _)| i)
                .collect();
            assert!(!led_ids.is_empty(), "fixture declares M4 LEDs");
            for i in led_ids {
                assert!(
                    !spec.cells.iter().any(|c| c.component_index == Some(i)),
                    "folded M4 LED publishes no cell"
                );
            }
        }
        let ini = "[b32]\n\n[button]\n    button = B2.1\n    led = L2.1\n    output = _X\n\n[copy]\n    input = _X\n    output = O1\n";
        let patch = crate::patch::Patch::from_ini_str(ini, "inline-b32".to_string()).unwrap();
        let mut app = App::new();
        assert!(app.load_patch(patch));
        assert_no_led_cells(&app, "B32 inline bound LED");
    }

    #[test]
    fn isotropic_mapping_maps_20x10mm_to_true_size() {
        let (rack, chain) = fixture();
        let geom = rack_geometry(&rack, &chain, &mapping());
        let (_, _, rect, _) = geom.cells[0];
        let ppm = crate::physical::PHYSICAL_POINTS_PER_MM as f32;
        assert!(
            (rect.width() - 20.0 * ppm).abs() < 1e-3,
            "width: {:?}",
            rect
        );
        assert!(
            (rect.height() - 10.0 * ppm).abs() < 1e-3,
            "height is isotropic: {:?}",
            rect
        );
    }

    #[test]
    fn mm_grid_pitch_is_isotropic_and_skeleton_matches_full() {
        let lines = mm_grid_lines(&mapping(), 200.0, 40.0, 20.0);
        let mut xs: Vec<f32> = lines
            .iter()
            .filter(|(a, b)| a.x == b.x)
            .map(|(a, _)| a.x)
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pitch = xs.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
        let step_pt = 20.0 * crate::physical::PHYSICAL_POINTS_PER_MM as f32;
        assert!(
            (pitch - step_pt).abs() < 1e-3,
            "vertical grid pitch isotropic: {xs:?}"
        );
        let mut ys: Vec<f32> = lines
            .iter()
            .filter(|(a, b)| a.y == b.y)
            .map(|(a, _)| a.y)
            .collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(!ys.is_empty(), "horizontal grid lines present");
        if ys.len() >= 2 {
            let hpitch = ys[1] - ys[0];
            assert!(
                (hpitch - step_pt).abs() < 1e-3,
                "horizontal grid pitch isotropic: {ys:?}"
            );
        }
        let app = app_with_patch("fixtures/own_buttons.ini");
        let mut full = app;
        full.physical_show_skeleton = false;
        let mut skel = app_with_patch("fixtures/own_buttons.ini");
        skel.physical_show_skeleton = true;
        let full_spec = physical_spec(&full, egui::Rect::ZERO).expect("spec");
        let skel_spec = physical_spec(&skel, egui::Rect::ZERO).expect("spec");
        assert_eq!(
            full_spec.cells.len(),
            skel_spec.cells.len(),
            "skeleton/full coincide"
        );
        let patch = full.patch.as_ref().expect("patch");
        for cell in &full_spec.cells {
            if let Some(i) = cell.component_index {
                assert_ne!(
                    patch.hw_components[i].kind,
                    ComponentKind::Led,
                    "no standalone LED cell"
                );
            }
        }
    }

    fn app_from_ini(ini: &str) -> App {
        let patch = crate::patch::Patch::from_ini_str(ini, "inline-test".to_string()).unwrap();
        let mut app = App::new();
        assert!(app.load_patch(patch));
        app
    }

    #[test]
    fn unaddressed_partial_b32_emits_dimmed_none_cells_with_no_hit_rect() {
        // UNREFERENCED-2.2-REDIRECT: the `[b32]`+`[button]` form lands B2.N
        // tokens on a fallback module (empty cells map), so no emission fix
        // can turn it green. The `[notebuttons]` entry form binds to the b32
        // geometry: 2 of 32 buttons addressed → 30 unused B cells dimmed.
        let ini = "[notebuttons]\n    button1 = B2.1\n    button2 = B2.2\n    led1 = L2.1\n    led2 = L2.2\n";
        let app = app_from_ini(ini);
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let dimmed_none: Vec<&CellSpec> = spec
            .cells
            .iter()
            .filter(|c| c.component_index.is_none() && c.dimmed)
            .collect();
        assert!(
            !dimmed_none.is_empty(),
            "partial B32: unused buttons emit dimmed None-index cells"
        );
        for c in &dimmed_none {
            assert!(c.led_state.is_none(), "unaddressed cell folds no LED state");
            assert!(c.led_rgb.is_none(), "unaddressed cell folds no LED rgb");
            assert!(c.shape.is_none(), "unaddressed cell has no composed shape");
        }
        let published: Vec<usize> = spec
            .cells
            .iter()
            .filter_map(|c| c.component_index)
            .collect();
        assert!(
            published.len() < spec.cells.len(),
            "dimmed None cells publish no hit rect (mirror of gui/mod.rs filter)"
        );
    }

    #[test]
    fn unaddressed_p2b8_led_emits_no_standalone_cell() {
        // UNREFERENCED-1.2-GUARD: pins that 2.2 suppression must not add
        // standalone L cells. Green today (LEDs without components never emit).
        let ini = "[p2b8]\n\n[button]\n    button = B1.1\n    led = L1.1\n    output = _A\n\n[copy]\n    input = _A\n    output = O1\n";
        let app = app_from_ini(ini);
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let folded: Vec<String> = patch
            .hw_components
            .iter()
            .filter(|c| c.kind == ComponentKind::Led)
            .map(|c| c.id.clone())
            .collect();
        assert!(!folded.is_empty(), "patch declares LEDs");
        for cell in &spec.cells {
            assert!(
                !cell.label.starts_with('L'),
                "no standalone L cell beyond the fold: {}",
                cell.label
            );
        }
    }

    #[test]
    fn bare_e4_emits_all_128_ring_segments() {
        let app = app_from_ini("[e4]\n");
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        assert!(
            spec.cells.len() >= 128,
            "bare E4 contains all 128 ring segments, got {}",
            spec.cells.len()
        );
    }

    #[test]
    fn bare_p2b8_spec_is_unchanged_no_unaddressed_cells() {
        // UNREFERENCED-1.2-GUARD: bare P2B8 full front stays byte-identical.
        // Green today; guards task 2.2.
        let app = app_from_ini("[p2b8]\n");
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        assert!(
            spec.cells.iter().all(|c| c.component_index.is_some()),
            "bare P2B8: no unaddressed cells"
        );
        let baseline = spec.cells.len();
        let app2 = app_from_ini("[p2b8]\n");
        let spec2 = physical_spec(&app2, egui::Rect::ZERO).expect("spec");
        assert_eq!(
            spec2.cells.len(),
            baseline,
            "bare P2B8 spec cell count stable"
        );
    }

    fn app_with_shift(path: &str, group: crate::patch::ShiftGroup, member_id: &str) -> App {
        let mut app = app_with_patch(path);
        let patch = app.patch.as_mut().expect("patch");
        let comp = patch
            .hw_components
            .iter_mut()
            .find(|c| c.id == member_id)
            .expect("member token in patch");
        comp.shift_group = Some(group);
        app.active_shift = Some(group);
        app
    }

    #[test]
    fn shift_member_composed_ring_base_and_slot_tint_group2() {
        use crate::patch::ShiftGroup;
        let app = app_with_shift("fixtures/own_buttons.ini", ShiftGroup::Group2, "B1.1");
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let idx = patch
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .expect("fixture declares B1.1");
        let cell = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .expect("B1.1 cell")
            .clone();
        let t = crate::theme::active();
        assert_eq!(
            cell.shift_color,
            Some(t.shift2),
            "member slot carries Group2"
        );
        let shapes = painted_shapes(&cell, false);
        let tint = rgb(t.shift2);
        let ring_hit = shapes.iter().any(|s| match s {
            egui::epaint::Shape::Circle(c) => {
                let expected = cell.rect.center();
                (c.center.x - expected.x).abs() < 0.5
                    && (c.center.y - expected.y).abs() < 0.5
                    && c.radius * 2.0 >= 0.8 * cell.rect.width().min(cell.rect.height())
                    && c.stroke.color == tint
            }
            _ => false,
        });
        assert!(ring_hit, "composed ring stroke tints Group2: {shapes:?}");
    }

    #[test]
    fn shift_member_glyph_colour_carries_group2_token() {
        use crate::patch::ShiftGroup;
        let ini = "[e4]\n\n[button]\n    button = B1.1\n    led = L1.1\n    output = _A\n\n[copy]\n    input = _A\n    output = O1\n";
        let mut app = app_from_ini(ini);
        app.patch
            .as_mut()
            .expect("patch")
            .hw_components
            .iter_mut()
            .find(|c| c.id == "B1.1")
            .expect("patch declares B1.1")
            .shift_group = Some(ShiftGroup::Group2);
        app.active_shift = Some(ShiftGroup::Group2);
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let idx = patch
            .hw_components
            .iter()
            .position(|c| c.id == "B1.1")
            .expect("patch declares B1.1");
        let cell = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .expect("B1.1 cell");
        assert!(cell.shape.is_none(), "E4 button keeps the glyph face");
        let t = crate::theme::active();
        assert_eq!(cell.color, t.shift2, "member glyph colour is Group2 token");
        assert_eq!(cell.shift_color, Some(t.shift2));
    }

    #[test]
    fn shift_non_member_spec_unchanged() {
        use crate::patch::ShiftGroup;
        let app = app_with_shift("fixtures/own_buttons.ini", ShiftGroup::Group2, "B1.1");
        let shifted = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let plain = app_with_patch("fixtures/own_buttons.ini");
        let baseline = physical_spec(&plain, egui::Rect::ZERO).expect("spec");
        let patch = app.patch.as_ref().expect("patch");
        let other_idx = patch
            .hw_components
            .iter()
            .position(|c| c.id != "B1.1" && c.kind == ComponentKind::Button)
            .expect("fixture has a second button");
        let cell = shifted
            .cells
            .iter()
            .find(|c| c.component_index == Some(other_idx))
            .expect("non-member cell");
        let base = baseline
            .cells
            .iter()
            .find(|c| c.component_index == Some(other_idx))
            .expect("baseline cell");
        assert_eq!(cell.color, base.color, "non-member colour unchanged");
        assert_eq!(cell.shift_color, None, "non-member slot stays None");
        assert_eq!(cell.glyph, base.glyph);
    }

    #[test]
    fn shift_no_shift_spec_byte_identical() {
        let a = app_with_patch("fixtures/own_buttons.ini");
        let b = app_with_patch("fixtures/own_buttons.ini");
        let sa = physical_spec(&a, egui::Rect::ZERO).expect("spec");
        let sb = physical_spec(&b, egui::Rect::ZERO).expect("spec");
        assert_eq!(sa.cells, sb.cells, "no-shift specs byte-identical");
        assert!(sa.cells.iter().all(|c| c.shift_color.is_none()));
    }

    #[test]
    fn shift_unaddressed_cells_stay_dim_none() {
        use crate::patch::ShiftGroup;
        let ini = "[notebuttons]\n    button1 = B2.1\n    button2 = B2.2\n    led1 = L2.1\n    led2 = L2.2\n";
        let mut app = app_from_ini(ini);
        app.active_shift = Some(ShiftGroup::Group2);
        let spec = physical_spec(&app, egui::Rect::ZERO).expect("spec");
        let dimmed_none: Vec<&CellSpec> = spec
            .cells
            .iter()
            .filter(|c| c.component_index.is_none() && c.dimmed)
            .collect();
        assert!(!dimmed_none.is_empty(), "partial B32 emits dimmed cells");
        for c in dimmed_none {
            assert_eq!(c.shift_color, None, "unaddressed cell never tints");
        }
    }

    // -- module-ui-labels 2.1 (red): label legibility -----------------------

    /// A patch loaded from `path` with `(token, label)` preamble labels
    /// attached, so the display-label chain (store -> layer 1 -> preamble ->
    /// derived) resolves exactly the strings these tests budget against
    /// without needing a dedicated fixture.
    fn app_with_labels(path: &str, labels: &[(&str, &str)]) -> App {
        let mut patch = crate::patch::Patch::from_ini_file(std::path::Path::new(path)).unwrap();
        for (token, label) in labels {
            patch
                .preamble_labels
                .insert((*token).to_string(), (*label).to_string());
        }
        let mut app = App::new();
        assert!(app.load_patch(patch));
        app
    }

    /// The global `patch.hw_components` index of `token`.
    fn hw_index(app: &App, token: &str) -> usize {
        app.patch
            .as_ref()
            .expect("patch")
            .hw_components
            .iter()
            .position(|c| c.id == token)
            .unwrap_or_else(|| panic!("token {token} exists"))
    }

    /// The drawn `CellSpec` for `token`, matched through the patch's global hw
    /// index (the same index the hit rects carry).
    fn cell_for_token<'a>(app: &App, spec: &'a PhysicalSpec, token: &str) -> &'a CellSpec {
        let idx = hw_index(app, token);
        spec.cells
            .iter()
            .find(|c| c.component_index == Some(idx))
            .unwrap_or_else(|| panic!("cell for {token}"))
    }

    /// module-ui-label-fit D1/D9, the compact path: the cell's layout is the
    /// annotation predicate — the label is fitted (wrapped, at a ladder step)
    /// wherever a 6 pt character fits, and the cell reports no layout only when
    /// even that cannot be drawn.
    #[test]
    fn cell_label_layout_matches_the_compact_cell_box() {
        let char_w = MONO_ADVANCE * CELL_LABEL_SIZE;
        let roomy = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(char_w * 5.0, 24.0));
        let short = cell_spec(roomy, "abc");
        assert!(
            short.label_layout.is_some(),
            "three characters fit the compact box: {:?}",
            short.label_layout
        );
        // A longer label is no longer "clipped": it wraps or ellipsizes in the
        // cell, so the cell still shows it.
        let longer = cell_spec(roomy, "abcdef");
        assert!(
            longer.label_layout.is_some(),
            "a longer label wraps instead of disappearing: {:?}",
            longer.label_layout
        );
        // Narrower than one 6 pt character of the text box: no layout at all,
        // so the cell is an annotation candidate.
        let narrow = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(char_w * 0.5, 24.0));
        let none = cell_spec(narrow, "a");
        assert!(
            none.label_layout.is_none(),
            "a cell narrower than one character has no layout: {:?}",
            none.label_layout
        );
        // An unlabelled cell (unaddressed geometry) has no layout either, and
        // is never an annotation candidate.
        assert!(cell_spec(roomy, "").label_layout.is_none());
    }

    /// module-ui-label-fit D6, the composed button-face path: the band beneath
    /// the shape — `h − 0.5 × w` — decides the layout, and it holds up to two
    /// lines. A face with no band keeps its shape and no label.
    #[test]
    fn cell_label_layout_matches_the_composed_band() {
        // A 9 mm P2B8/P4B2 button at 1:1 is 34 pt square: a 17 pt band.
        let square = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(34.0, 34.0));
        let mut fit = cell_spec(square, "Go");
        fit.shape = Some(CellShape::Round);
        let fit = resolved_cell(fit);
        assert!(
            fit.label_layout.is_some(),
            "a two-character label fits the button face: {:?}",
            fit.label_layout
        );
        // The same band holds a long label on two 6 pt lines rather than
        // dropping it: the face is never eaten by the label.
        let mut long = fit.clone();
        long.label = "Trigger input".to_string();
        let long = resolved_cell(long);
        let layout = long.label_layout.as_ref().expect("a two-line band");
        assert!(
            layout.lines.len() >= 2 && layout.font >= 6.0,
            "the long label wraps in the band: {layout:?}"
        );
        // The rect face (M4 touch plate, 12 x 18 mm) has a 45 pt band.
        let mut plate = long.clone();
        plate.shape = Some(CellShape::Rect);
        plate.rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(45.0, 68.0));
        assert!(
            resolved_cell(plate).label_layout.is_some(),
            "the M4 plate keeps the same label"
        );
        // A face taller than nothing: `h − 0.5 × w` is not positive, so there is
        // no band and no label — the cell stays an annotation candidate.
        let mut squat = cell_spec(
            Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(34.0, 16.0)),
            "Go",
        );
        squat.shape = Some(CellShape::Round);
        let squat = resolved_cell(squat);
        assert!(
            squat.label_layout.is_none(),
            "a face with no band hides its label: {:?}",
            squat.label_layout
        );
        assert!(cell_spec(square, "").label_layout.is_none());
    }

    /// module-ui-label-fit D5, the fader path: the label takes the column right
    /// of the narrowed track and stacks one glyph per line when that column
    /// cannot hold a line of text.
    #[test]
    fn cell_label_layout_matches_the_fader_column() {
        // A P8S8 fader is 4.2 mm (16 pt at 1:1): a 6.4 pt track leaves a 6.6 pt
        // column, so the label stacks instead of disappearing.
        let mut narrow = fader_spec(0.5);
        narrow.rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(16.0, 83.0));
        narrow.label = "Level".to_string();
        let narrow = resolved_cell(narrow);
        let layout = narrow.label_layout.as_ref().expect("a stacked column");
        assert!(
            layout.stacked && layout.lines.len() == 5,
            "one glyph per line beside the narrowed track: {layout:?}"
        );
        // A wide, tall fader (M4 motor fader at high zoom) keeps horizontal lines.
        let mut wide = narrow.clone();
        wide.rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(64.0, 234.0));
        wide.label = "Level".to_string();
        let wide = resolved_cell(wide);
        let layout = wide.label_layout.as_ref().expect("a horizontal column");
        assert!(
            !layout.stacked,
            "a wide fader keeps horizontal lines: {layout:?}"
        );
        let mut unlabelled = wide.clone();
        unlabelled.label = String::new();
        assert!(resolved_cell(unlabelled).label_layout.is_none());
    }

    /// module-ui-label-fit D1/D6 through the real spec: the composed face's band
    /// fits a short label at the top step and a long one on two fitted lines,
    /// so the layout — not a separate budget — says what the cell shows.
    #[test]
    fn physical_spec_fits_a_composed_label_per_cell() {
        let app = app_with_labels(
            "fixtures/own_buttons.ini",
            &[("B1.1", "Go"), ("B1.2", "Trigger input")],
        );
        let spec = physical_spec(&app, perf_pane()).expect("spec");
        let fits = cell_for_token(&app, &spec, "B1.1");
        assert_eq!(fits.label, "Go");
        let layout = fits
            .label_layout
            .as_ref()
            .unwrap_or_else(|| panic!("a short label fits the composed face: {fits:?}"));
        assert_eq!(layout.lines, vec!["Go".to_string()], "one top-step line");
        let long = cell_for_token(&app, &spec, "B1.2");
        assert_eq!(long.label, "Trigger input");
        let layout = long
            .label_layout
            .as_ref()
            .unwrap_or_else(|| panic!("the band holds the long label: {long:?}"));
        assert!(
            layout.lines.len() >= 2 && layout.font >= 6.0,
            "the band wraps it, never shrugs it off: {layout:?}"
        );
    }

    /// module-ui-label-fit D1 through the real spec: a DB8E button keeps the
    /// glyph path (no composed face), and its label layout is fitted across the
    /// whole cell because the glyph is a corner marker, not a line prefix.
    #[test]
    fn physical_spec_fits_a_compact_label_per_cell() {
        let app = app_with_labels(
            "fixtures/faceplate_rotary_stack.ini",
            &[("B6.1", "Go"), ("B6.2", "Trigger input")],
        );
        let spec = physical_spec(&app, perf_pane()).expect("spec");
        let fits = cell_for_token(&app, &spec, "B6.1");
        assert_eq!(fits.label, "Go");
        assert!(
            fits.shape.is_none(),
            "the DB8E button keeps the glyph path: {fits:?}"
        );
        assert!(
            fits.label_layout.is_some(),
            "a short label fits the cell: {fits:?}"
        );
        let long = cell_for_token(&app, &spec, "B6.2");
        assert_eq!(long.label, "Trigger input");
        let layout = long
            .label_layout
            .as_ref()
            .unwrap_or_else(|| panic!("a long label wraps in the cell: {long:?}"));
        assert!(
            layout.lines.len() >= 2,
            "the whole cell takes the wrapped label: {layout:?}"
        );
    }

    /// module-ui-label-fit D5 (task 3.6), through a loaded patch: the P8S8
    /// `P1.1` cell reaches the fader face. `is_fader` comes from the geometry
    /// cell's `F` family — the same fact `App`'s label fit classifies as
    /// `CellTextPath::Fader` — and the value from the element's own state, so the
    /// narrowed track, the amber bar and the stacked label column replace the
    /// compact cell a real patch used to paint.
    #[test]
    fn loaded_patch_p8s8_fader_cell_paints_the_fader_face_and_stacked_label() {
        let mut patch = crate::patch::Patch::from_ini_file(std::path::Path::new(
            "fixtures/module_ui_label_fit.ini",
        ))
        .expect("the label-fit fixture must parse");
        // A bare `[p8s8]` synthesizes its sliders at `Value(0.0)`: give the fader
        // a position so the state channel (the amber bar) has something to show.
        patch
            .hw_components
            .iter_mut()
            .find(|c| c.id == "P1.1")
            .expect("the bare [p8s8] declares P1.1")
            .state = crate::patch::ComponentState::Value(0.6);
        let mut app = App::new();
        assert!(app.load_patch(patch));
        let spec = physical_spec(&app, perf_pane()).expect("spec");
        let cell = cell_for_token(&app, &spec, "P1.1");
        assert_eq!(cell.label, "Volume", "the fixture's written fader label");
        assert!(
            cell.is_fader,
            "a P8S8 fader cell carries the geometry `F` family flag: {cell:?}"
        );
        assert!(
            (cell.fader_value - 0.6).abs() < 1e-6,
            "the bar follows the element state, not a guess: {}",
            cell.fader_value
        );
        // Fit and painter agree on the path: `App`'s label fit maps a geometry
        // `F` cell to `CellTextPath::Fader`, and the painter's own box — built
        // from the flag just asserted — now says the same.
        assert_eq!(
            cell_label_box(cell).path,
            crate::physical::CellTextPath::Fader,
            "the painted box is the fit's fader column"
        );

        let rect = cell.rect;
        let layout = cell
            .label_layout
            .as_ref()
            .unwrap_or_else(|| panic!("the fader column holds the label: {cell:?}"));
        assert!(
            layout.stacked,
            "a 4.2 mm fader column stacks one glyph per line: {layout:?}"
        );
        let fitted: Vec<&str> = layout.lines.iter().map(String::as_str).collect();
        assert_eq!(
            fitted,
            ["V", "o", "l", "u", "m", "e"],
            "one glyph per line: {layout:?}"
        );

        // The fader face: a track narrowed to 40 % of the cell, the slot outline,
        // the unlit part muted, and the amber value bar on the bottom edge.
        let rects = painted_cell_rects(cell, false);
        let track_w = (rect.width() * 0.4).min(10.0);
        let (slot, ..) = rects
            .iter()
            .find(|(_, _, stroke, color)| {
                *stroke == 1.0 && *color == rgb(crate::theme::active().fader_slot)
            })
            .unwrap_or_else(|| panic!("the fader slot outline is painted: {rects:?}"));
        assert!(
            (slot.width() - track_w).abs() < 0.6,
            "the track narrows to 40 % of the cell: {} vs {track_w}",
            slot.width()
        );
        let lit = rects
            .iter()
            .filter(|(_, fill, width, _)| {
                *width == 0.0 && *fill == rgb(crate::theme::active().fader_led_bar)
            })
            .max_by(|a, b| a.0.height().partial_cmp(&b.0.height()).unwrap())
            .map(|(r, ..)| *r)
            .unwrap_or_else(|| panic!("the amber value bar is painted: {rects:?}"));
        assert!(
            (lit.height() - rect.height() * 0.6).abs() < 1.0,
            "the bar fills 60 % of the track: {} vs {}",
            lit.height(),
            rect.height() * 0.6
        );
        assert!(
            (lit.max.y - rect.max.y).abs() < 0.6 && lit.min.x >= rect.min.x - 0.01,
            "the bar grows from the track's bottom edge: {lit:?}"
        );
        assert!(
            rects.iter().any(|(r, fill, width, _)| *width == 0.0
                && *fill == dim(rgb(crate::theme::active().muted), false)
                && (r.height() - rect.height() * 0.4).abs() < 1.0),
            "the unlit track above the bar stays muted: {rects:?}"
        );

        // The label: one monospace run per glyph, in the column right of the
        // track (a 3 pt gutter, `FADER_TRACK_GUTTER`), top to bottom, never below
        // the 6 pt floor.
        let runs = label_lines(cell);
        assert_eq!(runs.len(), 6, "one run per stacked glyph: {runs:?}");
        let painted: String = runs.iter().map(|(text, _, _)| text.as_str()).collect();
        assert_eq!(painted, "Volume", "every glyph in reading order: {runs:?}");
        let mut ys: Vec<f32> = Vec::new();
        for (text, size, pos) in &runs {
            assert_eq!(text.chars().count(), 1, "one glyph per line: {text:?}");
            assert_ladder_step(*size, "loaded fader");
            assert!(
                pos.x >= rect.min.x + track_w + 3.0 - 0.5,
                "the column sits beside the narrowed track: {pos:?}"
            );
            ys.push(pos.y);
        }
        let mut sorted = ys.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(ys, sorted, "glyphs stack top to bottom: {ys:?}");
        assert_floor_respected(cell, "loaded fader");

        // The state row appears exactly when the layout kept room for it (D4).
        // It is clipped to the column it shares with the stacked label, and in a
        // column one character wide the clip marker is all that can be drawn —
        // what matters here is that the row follows the layout, not the cell.
        let state = state_lines(cell);
        if layout.state_row {
            assert_eq!(state.len(), 1, "the value row survives: {state:?}");
            assert!(
                cell.state_text.starts_with(state[0].as_str())
                    || state[0].chars().all(|c| c == '\u{2026}'),
                "the row carries the state, or the clip marker a one-character column allows: {state:?}"
            );
        } else {
            assert!(
                state.is_empty(),
                "no state row once the label takes its line: {state:?}"
            );
        }
    }

    /// Run the real pane dispatch once over an 800 x 600 band, so
    /// `app.component_rects` is published exactly as at runtime (the same
    /// entry point the window loop uses).
    fn run_paint_panes(app: &mut App) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let win = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let raw = egui::RawInput {
            screen_rect: Some(win),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            super::super::paint_panes(app, ui, None, &[]);
        });
        out.textures_delta.clear();
        out
    }

    /// The module UI pane rect the last paint published, as an egui rect.
    fn published_physical_pane(app: &App) -> Rect {
        let (_, cell) = app
            .pane_hit_rects
            .iter()
            .find(|(id, _)| *id == crate::panes::PaneId::BigLeft)
            .expect("the module UI pane is published");
        Rect::from_min_size(
            Pos2::new(cell.x as f32, cell.y as f32),
            Vec2::new(cell.width as f32, cell.height as f32),
        )
    }

    /// module-ui-label-fit D9/`auto`: the presentation annotates exactly the
    /// addressed elements whose label has no in-cell layout. At 1:1 the ladder
    /// fits every written label of this fixture, so the layer stays silent and
    /// a label the cell already draws is never duplicated; `on` still draws the
    /// 14 pt monospace callout outside the rack with one leader line each.
    #[test]
    fn auto_annotates_exactly_the_cells_with_no_in_cell_layout() {
        let labels = &[("B1.1", "Go"), ("B1.2", "Trigger input")];
        let mut off = app_with_labels("fixtures/own_buttons.ini", labels);
        off.label_presentation = LabelPresentation::Off;
        let off_spec = physical_spec(&off, perf_pane()).expect("spec");

        let mut auto = app_with_labels("fixtures/own_buttons.ini", labels);
        auto.label_presentation = LabelPresentation::Auto;
        let spec = physical_spec(&auto, perf_pane()).expect("spec");
        // The predicate, cell by cell: `auto` selects only cells with no layout.
        for cell in spec
            .cells
            .iter()
            .filter(|cell| cell.component_index.is_some() && !cell.label.is_empty())
        {
            assert!(
                cell.label_layout.is_some(),
                "the ladder fits '{}' in its own cell at 1:1: {cell:?}",
                cell.label
            );
        }
        assert!(
            spec.annotations.is_empty(),
            "nothing lacks an in-cell layout, so nothing is annotated: {:?}",
            spec.annotations
        );
        for c in &spec.annotations {
            assert!(
                rects_disjoint(&c.label_rect, &spec.case_rect),
                "annotation '{}' enters the rack: {:?}",
                c.label,
                c.label_rect
            );
        }

        // `on` is unchanged: the same labels are annotated, drawn at the
        // performance view's callout size with one leader line each.
        let mut on = app_with_labels("fixtures/own_buttons.ini", labels);
        on.label_presentation = LabelPresentation::On;
        let on_spec = physical_spec(&on, perf_pane()).expect("spec");
        let texts = painted_texts(&on_spec);
        assert!(
            texts.iter().any(|(t, font, _)| t == "Trigger input"
                && *font == egui::FontId::monospace(CALLOUT_LABEL_SIZE)),
            "the annotation headline paints big: {texts:?}"
        );
        for c in &on_spec.annotations {
            assert!(
                rects_disjoint(&c.label_rect, &on_spec.case_rect),
                "annotation '{}' enters the rack: {:?}",
                c.label,
                c.label_rect
            );
        }
        assert_eq!(
            painted_line_count(&on_spec),
            painted_line_count(&off_spec) + on_spec.annotations.len(),
            "one leader line per annotation"
        );
    }

    /// module-ui-labels D10: `off` keeps the cells alone (no annotation at
    /// all), and `on` annotates every addressed element that carries a label
    /// with no dropping.
    #[test]
    fn label_presentation_off_annotates_nothing_and_on_annotates_every_labelled_element() {
        let labels = &[("B1.1", "Go"), ("B1.2", "Trigger input")];

        let mut off = app_with_labels("fixtures/own_buttons.ini", labels);
        off.label_presentation = LabelPresentation::Off;
        let off_spec = physical_spec(&off, perf_pane()).expect("spec");
        assert!(
            off_spec.annotations.is_empty(),
            "off draws no annotation: {:?}",
            off_spec.annotations
        );
        let off_texts = painted_texts(&off_spec);
        assert!(
            !off_texts
                .iter()
                .any(|(_, font, _)| font.family == egui::FontFamily::Monospace
                    && font.size == CALLOUT_LABEL_SIZE),
            "no callout-sized text in off: {off_texts:?}"
        );

        let mut on = app_with_labels("fixtures/own_buttons.ini", labels);
        on.label_presentation = LabelPresentation::On;
        let on_spec = physical_spec(&on, perf_pane()).expect("spec");
        let labelled: Vec<&CellSpec> = on_spec
            .cells
            .iter()
            .filter(|c| c.component_index.is_some() && !c.label.is_empty())
            .collect();
        assert!(
            labelled.len() >= 8,
            "the fixture addresses eight labelled buttons: {}",
            labelled.len()
        );
        assert_eq!(
            on_spec.annotations.len(),
            labelled.len(),
            "on annotates every labelled addressed element and drops none"
        );
        for cell in labelled {
            assert!(
                on_spec.annotations.iter().any(|a| a.label == cell.label),
                "annotated '{}'",
                cell.label
            );
        }
    }

    /// module-ui-label-fit D9: an element the controller declares but the patch
    /// does not address carries no label — even when a preamble label exists for
    /// the token — so it has no in-cell layout and no presentation annotates it.
    #[test]
    fn auto_never_annotates_an_unaddressed_element() {
        // Fixture gap: `fixtures/unreferenced_elements.ini` is a partial B32 too,
        // but it also carries a bare `[e4]` whose 7 mm encoder, 3 mm button and
        // 1.8 mm ring cells have no in-cell layout at 1:1, so it cannot isolate
        // "an unaddressed element is never annotated". This inline patch is the
        // same partial B32 another test in this module uses (`[notebuttons]`,
        // 2 of 32).
        let ini = "[notebuttons]\n    button1 = B2.1\n    button2 = B2.2\n    led1 = L2.1\n    led2 = L2.2\n";
        let mut patch =
            crate::patch::Patch::from_ini_str(ini, "inline-notebuttons".to_string()).unwrap();
        // A preamble label for a token the patch never addresses, plus a long
        // label on an addressed button so the addressed set is non-empty.
        patch
            .preamble_labels
            .insert("B2.5".to_string(), "Unaddressed long label".to_string());
        patch
            .preamble_labels
            .insert("B2.1".to_string(), "Start sequencer".to_string());
        let mut app = App::new();
        assert!(app.load_patch(patch));
        app.label_presentation = LabelPresentation::Auto;
        let spec = physical_spec(&app, perf_pane()).expect("spec");
        let unaddressed: Vec<&CellSpec> = spec
            .cells
            .iter()
            .filter(|c| c.component_index.is_none())
            .collect();
        assert!(
            unaddressed.iter().any(|c| c.dimmed),
            "the partial B32 leaves dimmed unaddressed cells"
        );
        for cell in &unaddressed {
            assert!(
                cell.label.is_empty(),
                "an unaddressed cell carries no label: {cell:?}"
            );
            assert!(
                cell.label_layout.is_none(),
                "and therefore no label layout: {cell:?}"
            );
        }
        assert!(
            spec.annotations
                .iter()
                .all(|a| a.label != "Unaddressed long label"),
            "an unaddressed element is never annotated: {:?}",
            spec.annotations
        );
        for a in &spec.annotations {
            assert!(
                spec.cells
                    .iter()
                    .any(|c| c.component_index.is_some() && c.rect.contains(a.anchor)),
                "annotation '{}' is anchored on an addressed cell: {:?}",
                a.label,
                a.anchor
            );
        }
    }

    /// module-ui-labels D10, the 30 % band budget and the fixed drop order:
    /// LEDs and jacks drop first, then buttons, switches, pots, and faders
    /// last, and a protected (focused / selected-circuit emphasis) candidate
    /// is never dropped.
    #[test]
    fn annotation_budget_drops_in_family_order_and_keeps_protected_cells() {
        let pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(400.0, 300.0));
        let rack = Rect::from_min_size(Pos2::new(150.0, 120.0), Vec2::new(100.0, 50.0));
        // Every host sits above the rack centre, so all seven candidates bind
        // to the Over side; each six-character headline is wider than half the
        // rack, so each gets its own band (`depth = 4 + k * 25.5 + (k-1) * 2`
        // for k bands with an empty state row).
        let candidate =
            |kind: ComponentKind, is_fader: bool, label: &str, x: f32, highlighted: bool| {
                let mut cell = cell_spec(
                    Rect::from_min_size(Pos2::new(x, 20.0), Vec2::new(20.0, 9.0)),
                    label,
                );
                cell.kind = kind;
                cell.is_fader = is_fader;
                cell.highlighted = highlighted;
                cell.state_text = String::new();
                cell
            };
        let led = candidate(ComponentKind::Led, false, "LedTop", 150.0, false);
        let jack = candidate(ComponentKind::CvIn, false, "JackCV", 160.0, false);
        let button = candidate(ComponentKind::Button, false, "Button", 170.0, false);
        let switch = candidate(ComponentKind::Switch, false, "Switch", 180.0, false);
        let pot = candidate(ComponentKind::Knob, false, "PotAAA", 190.0, false);
        let fader = candidate(ComponentKind::Knob, true, "FaderX", 200.0, false);
        let protected = candidate(ComponentKind::Led, false, "ProtLD", 210.0, true);
        let cells: [&CellSpec; 7] = [&led, &jack, &button, &switch, &pot, &fader, &protected];

        // Three bands fit in 300 pt (30 % = 90 pt; k = 3 is 84.5 pt, k = 4 is
        // 112 pt), so four labels drop: both LEDs/jacks first, then the button
        // and the switch. The protected LED and the fader stay.
        let plan = budget_annotations(pane, rack, &cells);
        assert_eq!(plan.dropped, 4, "four labels exceed the 30 % band budget");
        let mut kept: Vec<&str> = plan.callouts.iter().map(|c| c.label.as_str()).collect();
        kept.sort_unstable();
        assert_eq!(
            kept,
            vec!["FaderX", "PotAAA", "ProtLD"],
            "LEDs, jacks, buttons and switches drop before pots and faders"
        );
        assert!(
            plan.callouts
                .iter()
                .any(|c| c.label == "ProtLD" && c.highlighted),
            "the protected element keeps its callout"
        );
        for c in &plan.callouts {
            assert!(
                rects_disjoint(&c.label_rect, &rack),
                "kept annotation '{}' enters the rack: {:?}",
                c.label,
                c.label_rect
            );
        }

        // A pane tall enough for every band drops nothing.
        let tall = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(400.0, 800.0));
        let plan = budget_annotations(tall, rack, &cells);
        assert_eq!(plan.dropped, 0, "a tall pane drops nothing");
        assert_eq!(plan.callouts.len(), 7, "every candidate is annotated");
    }

    /// module-ui-labels D10: annotations are decoration. Switching the label
    /// presentation between `off`, `auto`, and `on` leaves the published
    /// component hit rects, the drawn cells, and the skeleton rects identical.
    #[test]
    fn switching_label_presentation_leaves_hit_and_skeleton_rects_identical() {
        type Geometry = (Rect, Vec<Rect>, Vec<Rect>);
        let mut hits: Vec<Vec<(usize, crate::app::Rect)>> = Vec::new();
        let mut full: Vec<Geometry> = Vec::new();
        let mut skeleton: Vec<Geometry> = Vec::new();
        for mode in [
            LabelPresentation::Off,
            LabelPresentation::Auto,
            LabelPresentation::On,
        ] {
            let mut app = app_with_labels(
                "fixtures/own_buttons.ini",
                &[("B1.1", "Go"), ("B1.2", "Trigger input")],
            );
            app.label_presentation = mode;
            let mut out = run_paint_panes(&mut app);
            hits.push(app.component_rects.clone());
            let pane = published_physical_pane(&app);
            let geometry = |spec: &PhysicalSpec| -> Geometry {
                (
                    spec.case_rect,
                    spec.modules.iter().map(|m| m.rect).collect(),
                    spec.cells.iter().map(|c| c.rect).collect(),
                )
            };
            app.physical_show_skeleton = false;
            let full_spec = physical_spec(&app, pane).expect("full spec");
            full.push(geometry(&full_spec));
            app.physical_show_skeleton = true;
            let skel_spec = physical_spec(&app, pane).expect("skeleton spec");
            skeleton.push(geometry(&skel_spec));
            out.textures_delta.clear();
        }
        assert!(
            !hits[0].is_empty(),
            "the module UI publishes component hit rects"
        );
        assert_eq!(hits[0], hits[1], "off and auto publish identical hit rects");
        assert_eq!(hits[0], hits[2], "off and on publish identical hit rects");
        assert_eq!(full[0], full[1], "off and auto draw identical cells");
        assert_eq!(full[0], full[2], "off and on draw identical cells");
        assert_eq!(
            skeleton[0], skeleton[1],
            "off and auto draw identical skeleton rects"
        );
        assert_eq!(
            skeleton[0], skeleton[2],
            "off and on draw identical skeleton rects"
        );
    }

    /// module-ui-labels D11: hovering an addressed element reveals its full
    /// label and state as a callout anchored to that cell, in every label
    /// presentation and in both the full and skeleton presentations; with no
    /// hovered element nothing is revealed.
    #[test]
    fn hover_reveal_shows_the_full_label_and_state_in_every_mode() {
        for mode in [
            LabelPresentation::Off,
            LabelPresentation::Auto,
            LabelPresentation::On,
        ] {
            for skeleton in [false, true] {
                let mut app = app_with_labels(
                    "fixtures/own_buttons.ini",
                    &[("B1.1", "Gate trigger input")],
                );
                app.label_presentation = mode;
                app.physical_show_skeleton = skeleton;

                // No hover: no reveal.
                app.hovered_component = None;
                let spec = physical_spec(&app, perf_pane()).expect("spec");
                assert!(
                    spec.hover.is_none(),
                    "no callout without a hovered element ({mode:?}, skeleton={skeleton})"
                );

                // Hover the clipped-label button: the full label plus state.
                let hw = hw_index(&app, "B1.1");
                app.hovered_component = Some(hw);
                let spec = physical_spec(&app, perf_pane()).expect("spec");
                let cell_rect = spec
                    .cells
                    .iter()
                    .find(|c| c.component_index == Some(hw))
                    .expect("hovered cell")
                    .rect;
                let hover = spec.hover.as_ref().unwrap_or_else(|| {
                    panic!("hover callout in {mode:?} with skeleton={skeleton}")
                });
                assert_eq!(
                    hover.label, "Gate trigger input",
                    "the full label, not the in-cell ellipsis"
                );
                assert!(
                    !hover.state_text.is_empty(),
                    "the callout carries the state row: {hover:?}"
                );
                assert!(
                    rects_disjoint(&hover.label_rect, &cell_rect),
                    "the callout never covers the cell it describes: {:?}",
                    hover.label_rect
                );
                assert!(
                    hover.anchor.x >= cell_rect.min.x - 1.0
                        && hover.anchor.x <= cell_rect.max.x + 1.0
                        && hover.anchor.y >= cell_rect.min.y - 1.0
                        && hover.anchor.y <= cell_rect.max.y + 1.0,
                    "the callout is anchored to the hovered cell edge: {:?}",
                    hover.anchor
                );
                let texts = painted_texts(&spec);
                assert!(
                    texts.iter().any(|(t, font, _)| t == "Gate trigger input"
                        && *font == egui::FontId::monospace(CALLOUT_LABEL_SIZE)),
                    "the reveal paints at {CALLOUT_LABEL_SIZE} pt: {texts:?}"
                );
            }
        }
    }

    /// module-ui-labels D11: a hovered cell at the pane's right edge gets its
    /// callout on the side that keeps it inside the pane.
    #[test]
    fn hover_callout_flips_side_to_stay_inside_the_pane() {
        // A P2B8 rack is 25.4 mm (96 pt) wide; a pane that wide leaves the
        // right button column (57-92 pt) no room to its right.
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(96.0, 600.0));
        let mut app = app_with_labels("fixtures/own_buttons.ini", &[("B1.2", "Trig")]);
        app.label_presentation = LabelPresentation::Off;
        let hw = hw_index(&app, "B1.2");
        app.hovered_component = Some(hw);
        let spec = physical_spec(&app, pane).expect("spec");
        let cell_rect = spec
            .cells
            .iter()
            .find(|c| c.component_index == Some(hw))
            .expect("hovered cell")
            .rect;
        let hover = spec.hover.as_ref().expect("hover callout");
        assert!(
            hover.label_rect.min.x >= pane.min.x - 1.0
                && hover.label_rect.max.x <= pane.max.x + 1.0,
            "the callout stays inside the pane: {:?}",
            hover.label_rect
        );
        assert!(
            hover.label_rect.max.x <= cell_rect.min.x + 1.0,
            "the callout flips left of the cell: {:?} vs {cell_rect:?}",
            hover.label_rect
        );
    }
    // -- module-ui-label-fit 1.1 (red): fitted label layout -------------------
    //
    // `label_layout` (D1) only exists from task 1.2, so these tests pin the
    // contract through the paint surface the module already has: a cell is
    // painted headless and the emitted `Shape::Text` runs are read back as the
    // label lines (monospace, state glyph stripped), the state row
    // (proportional), and the font step each carries. What they pin (D2-D6):
    //
    // - the label is fitted to the cell's own text box, largest step of
    //   `10, 9, 8, 7, 6` first; the state glyph is not charged against it;
    // - greedy word wrap happens *before* a smaller step is tried;
    // - a word wider than the line is hard-broken (characters kept, no `…`);
    // - `…` appears only at the 6 pt floor, and only when the whole label
    //   cannot be drawn there;
    // - the state row keeps its line when a step leaves room, and yields the
    //   cell when none does;
    // - the composed face keeps its shape and gives the label the band beneath
    //   it (one and two lines);
    // - the P8S8 fader narrows its track to 40 % and stacks one glyph per line;
    // - a cell with no layout paints no in-cell label at all.

    /// The descending label ladder (D2/D3) and its floor.
    const LABEL_LADDER: [f32; 5] = [10.0, 9.0, 8.0, 7.0, 6.0];
    const LABEL_FLOOR: f32 = 6.0;

    /// Millimetres to points at the 1:1 reference: the true size of a geometry
    /// cell, the same factor `rack_geometry` applies at zoom 1.
    fn pt(mm: f32) -> f32 {
        mm * crate::physical::PHYSICAL_POINTS_PER_MM as f32
    }

    /// One painted text run: its text, its font, and its painted origin/size.
    #[derive(Debug, Clone)]
    struct PaintedRun {
        text: String,
        font: egui::FontId,
        pos: Pos2,
        size: Vec2,
    }

    /// Paint one cell headless (full presentation) and return every
    /// `Shape::Text` run with the geometry the label assertions need.
    fn painted_runs(cell: &CellSpec) -> Vec<PaintedRun> {
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
            paint_cell(painter, cell, false, false);
        });
        let mut runs = Vec::new();
        for cs in &full_output.shapes {
            if let egui::epaint::Shape::Text(t) = &cs.shape {
                runs.push(PaintedRun {
                    text: t.galley.text().to_string(),
                    font: t.galley.job.sections[0].format.font_id.clone(),
                    pos: t.pos,
                    size: t.galley.size(),
                });
            }
        }
        full_output.textures_delta.clear();
        runs
    }

    /// The label lines a cell paints, in top-to-bottom paint order: every
    /// monospace run with the state glyph and surrounding space removed, plus
    /// its font step and painted origin. Empty (a glyph-only row, an empty
    /// label) runs are not label lines.
    fn label_lines(cell: &CellSpec) -> Vec<(String, f32, Pos2)> {
        let glyph = cell.glyph.as_str();
        let mut lines: Vec<(String, f32, Pos2)> = painted_runs(cell)
            .into_iter()
            .filter(|run| run.font.family == egui::FontFamily::Monospace)
            .filter_map(|run| {
                let text = run.text.trim();
                let text = if glyph.is_empty() {
                    text
                } else {
                    text.strip_prefix(glyph).unwrap_or(text)
                };
                let text = text.trim();
                (!text.is_empty()).then(|| (text.to_string(), run.font.size, run.pos))
            })
            .collect();
        lines.sort_by(|a, b| {
            a.2.y
                .partial_cmp(&b.2.y)
                .unwrap()
                .then(a.2.x.partial_cmp(&b.2.x).unwrap())
        });
        lines
    }

    /// The painted label text, line by line, rejoined with single spaces.
    fn label_text(cell: &CellSpec) -> String {
        label_lines(cell)
            .iter()
            .map(|(text, _, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The proportional runs a cell paints: its in-cell state row.
    fn state_lines(cell: &CellSpec) -> Vec<String> {
        painted_runs(cell)
            .into_iter()
            .filter(|run| run.font.family == egui::FontFamily::Proportional)
            .map(|run| run.text.trim().to_string())
            .filter(|text| !text.is_empty())
            .collect()
    }

    fn assert_ladder_step(size: f32, ctx: &str) {
        assert!(
            LABEL_LADDER.contains(&size),
            "{ctx}: {size} pt is not a ladder step"
        );
    }

    fn assert_no_ellipsis(cell: &CellSpec, ctx: &str) {
        for (text, _, _) in label_lines(cell) {
            assert!(
                !text.contains('\u{2026}'),
                "{ctx}: the whole label fits, so no ellipsis: {text:?}"
            );
        }
    }

    /// The legibility floor (D3/D10): no text run anywhere below 6 pt.
    fn assert_floor_respected(cell: &CellSpec, ctx: &str) {
        for run in painted_runs(cell) {
            assert!(
                run.font.size >= LABEL_FLOOR,
                "{ctx}: {:?} painted at {} pt, below the {LABEL_FLOOR} pt floor",
                run.text,
                run.font.size
            );
        }
    }

    /// A compact full-presentation cell at true panel size, with the glyph and
    /// state text the element's kind renders and its label layout resolved for
    /// that glyph/state pair.
    fn sized_cell(w_mm: f32, h_mm: f32, glyph: &str, label: &str, state: &str) -> CellSpec {
        let mut cell = cell_spec(
            Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(pt(w_mm), pt(h_mm))),
            label,
        );
        cell.glyph = glyph.to_string();
        cell.state_text = state.to_string();
        resolved_cell(cell)
    }

    /// A 12 mm pot (45.4 pt at 1:1): the roomiest element cell on the panel.
    fn pot_cell(label: &str) -> CellSpec {
        sized_cell(12.0, 12.0, "\u{25C9}", label, "50%")
    }

    /// A 7 mm jack (26.5 pt at 1:1): one 10 pt character is 6 pt, so the cell is
    /// only a few characters wide.
    fn jack_cell(label: &str) -> CellSpec {
        sized_cell(7.0, 7.0, "\u{25C0}", label, "CV IN")
    }

    /// A composed button face: shape only, no glyph and no state row (D6).
    fn composed_cell(w_mm: f32, h_mm: f32, label: &str, shape: CellShape) -> CellSpec {
        let mut cell = sized_cell(w_mm, h_mm, "", label, "");
        cell.shape = Some(shape);
        cell.led_state = Some(1.0);
        resolved_cell(cell)
    }

    /// The P8S8 fader cell: 4.2 x 22 mm (15.9 x 83.1 pt at 1:1), label and value.
    fn p8s8_fader_cell(label: &str) -> CellSpec {
        let mut cell = sized_cell(4.2, 22.0, "", label, "50%");
        cell.is_fader = true;
        cell.fader_value = 0.5;
        cell.kind = ComponentKind::Knob;
        cell.color = crate::theme::active().fader_led_bar;
        cell.label_color = crate::theme::active().fader_led_bar;
        resolved_cell(cell)
    }

    /// module-ui-label-fit D2: a roomy 12 mm pot reaches the ladder's top step.
    /// Six characters are 36 pt of a 45.4 pt line, so the whole label holds at
    /// 10 pt — the fixed font's glyph-plus-space budget only afforded five.
    #[test]
    fn wide_pot_takes_the_largest_ladder_step() {
        let cell = pot_cell("Volume");
        let lines = label_lines(&cell);
        assert_eq!(
            lines.len(),
            1,
            "a six-character label holds on one top-step line: {lines:?}"
        );
        assert_eq!(lines[0].0, "Volume", "the whole label: {lines:?}");
        assert_eq!(lines[0].1, LABEL_LADDER[0], "the 10 pt step: {lines:?}");
        assert_no_ellipsis(&cell, "wide pot");
        assert_floor_respected(&cell, "wide pot");
    }

    /// module-ui-label-fit D2/D3 (spec "A small cell takes a smaller step"): a
    /// 7 mm jack drops below the 10 pt cell font, stays on the ladder, and never
    /// goes under the 6 pt floor. Six characters need 36 pt at 10 pt against a
    /// 26.5 pt cell, so no larger step holds the word whole.
    #[test]
    fn narrow_jack_takes_a_smaller_ladder_step_and_never_below_the_floor() {
        let cell = jack_cell("Reverb");
        let lines = label_lines(&cell);
        assert_eq!(
            lines.len(),
            1,
            "one word on the step that holds it: {lines:?}"
        );
        assert_eq!(lines[0].0, "Reverb", "the whole label: {lines:?}");
        let size = lines[0].1;
        assert_ladder_step(size, "narrow jack");
        assert!(
            size < CELL_LABEL_SIZE,
            "a step below the 10 pt cell font: {lines:?}"
        );
        assert!(size >= LABEL_FLOOR, "never below the floor: {lines:?}");
        assert!(
            ("Reverb".chars().count() as f32) * MONO_ADVANCE * size <= cell.rect.width() + 0.5,
            "the line fits the cell width at {size} pt: {lines:?}"
        );
        assert_no_ellipsis(&cell, "narrow jack");
        assert_floor_respected(&cell, "narrow jack");
    }

    /// module-ui-label-fit D2 (spec "Wrap before shrink"): "Master out" is ten
    /// characters, so no ladder step holds it on one 45.4 pt line (10 pt needs
    /// 60 pt). Reaching the 10 pt step therefore requires wrapping at 10 pt
    /// instead of stepping the font down — the assertion that fails while the
    /// cell's single fixed line can only ellipsize.
    #[test]
    fn label_wraps_at_the_current_step_before_stepping_down() {
        let cell = pot_cell("Master out");
        let lines = label_lines(&cell);
        assert!(
            lines.len() >= 2,
            "wrapped into lines, not one clipped line: {lines:?}"
        );
        for (text, size, _) in &lines {
            assert_eq!(
                *size, LABEL_LADDER[0],
                "wrapping keeps the 10 pt step for {text:?}: {lines:?}"
            );
        }
        assert_eq!(
            label_text(&cell),
            "Master out",
            "the whole label: {lines:?}"
        );
        assert_no_ellipsis(&cell, "wrap before shrink");
        assert_floor_respected(&cell, "wrap before shrink");
    }

    /// module-ui-label-fit D2: a single word wider than the line is hard-broken
    /// with every character kept — no ellipsis, because an ellipsis is reserved
    /// for the floor (D2).
    #[test]
    fn over_long_word_hard_breaks_instead_of_ellipsizing() {
        // A 5 x 14 mm switch cell: 18.9 pt wide, so "Rotaryswitch" can never sit
        // on one line at any step (twelve 6 pt characters are 43 pt).
        let cell = sized_cell(5.0, 14.0, "\u{25A3}", "Rotaryswitch", "ON");
        let lines = label_lines(&cell);
        assert!(
            lines.len() >= 2,
            "the over-long word breaks across lines: {lines:?}"
        );
        assert_eq!(
            label_text(&cell).replace(' ', ""),
            "Rotaryswitch",
            "every character is kept, none dropped: {lines:?}"
        );
        assert_no_ellipsis(&cell, "hard break");
        for (text, size, _) in &lines {
            assert_ladder_step(*size, "hard break");
            assert!(
                (text.chars().count() as f32) * MONO_ADVANCE * size <= cell.rect.width() + 0.5,
                "line {text:?} fits the cell width at {size} pt"
            );
        }
        assert_floor_respected(&cell, "hard break");
    }

    /// module-ui-label-fit D2/D3: the ellipsis is the floor's last resort — a
    /// 23-character label on a 7 mm jack cannot be drawn whole even at 6 pt (the
    /// cell holds three seven-character lines without the state row, 21
    /// characters), so the ladder reaches 6 pt and ellipsizes the last line.
    #[test]
    fn ellipsis_appears_only_at_the_six_point_floor() {
        let cell = jack_cell("Filter cutoff resonance");
        let lines = label_lines(&cell);
        assert!(
            !lines.is_empty(),
            "the floor still draws the label: {lines:?}"
        );
        for (text, size, _) in &lines {
            assert_eq!(
                *size, LABEL_FLOOR,
                "the ellipsis only ever appears at the floor, not at {size} pt ({text:?})"
            );
        }
        let ellipsized: Vec<&(String, f32, Pos2)> = lines
            .iter()
            .filter(|(text, _, _)| text.contains('\u{2026}'))
            .collect();
        assert_eq!(
            ellipsized.len(),
            1,
            "exactly one ellipsis, on the last line: {lines:?}"
        );
        assert!(
            lines.last().unwrap().0.contains('\u{2026}'),
            "the ellipsis closes the label: {lines:?}"
        );
        let visible: String = lines
            .iter()
            .map(|(text, _, _)| text.replace('\u{2026}', ""))
            .collect::<Vec<_>>()
            .join("")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let whole: String = "Filter cutoff resonance"
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(
            whole.starts_with(&visible),
            "the visible text is the label's prefix, never scrambled: {visible:?}"
        );
        assert!(
            visible.chars().count() < whole.chars().count(),
            "the whole label does not fit the floor: {visible:?}"
        );
        assert_floor_respected(&cell, "floor ellipsis");
    }

    /// module-ui-label-fit D4: a shallow cell still clears one 10 pt label line
    /// (12.5 pt) plus the 9 pt state row (11.25 pt) in 26.5 pt, so the state row
    /// keeps its line under the label.
    #[test]
    fn state_row_survives_when_the_fitted_label_leaves_room() {
        let cell = sized_cell(12.0, 7.0, "\u{25C9}", "Volume", "50%");
        let lines = label_lines(&cell);
        assert_eq!(lines.len(), 1, "one 10 pt label line: {lines:?}");
        assert_eq!(lines[0].0, "Volume", "the whole label: {lines:?}");
        assert_eq!(lines[0].1, LABEL_LADDER[0], "the top step: {lines:?}");
        assert_eq!(
            state_lines(&cell),
            vec!["50%".to_string()],
            "the state row survives beside a label that leaves room"
        );
        let runs = painted_runs(&cell);
        let label_run = runs
            .iter()
            .find(|run| run.font.family == egui::FontFamily::Monospace)
            .expect("label line painted");
        let state_run = runs
            .iter()
            .find(|run| run.font.family == egui::FontFamily::Proportional)
            .expect("state row painted");
        assert!(
            state_run.pos.y > label_run.pos.y,
            "the state row sits under the label line: {:?} vs {:?}",
            state_run.pos,
            label_run.pos
        );
        assert_no_ellipsis(&cell, "state row survives");
        assert_floor_respected(&cell, "state row survives");
    }

    /// module-ui-label-fit D4 (spec "The label beats the state row"): on a 9 mm
    /// button cell a 23-character label cannot fit alongside the state row at any
    /// step, but it fits the whole cell at the floor — so the label takes the
    /// cell and the state row is omitted.
    #[test]
    fn state_row_yields_the_cell_when_no_step_fits_alongside_it() {
        let cell = sized_cell(9.0, 9.0, "\u{25CF}", "Filter cutoff resonance", "ON");
        let lines = label_lines(&cell);
        assert!(
            lines.len() >= 3,
            "the label takes the cell the state row gave up: {lines:?}"
        );
        assert_eq!(
            label_text(&cell).replace(' ', ""),
            "Filtercutoffresonance",
            "the whole label, unellipsized: {lines:?}"
        );
        assert_no_ellipsis(&cell, "state row yields");
        for (_, size, _) in &lines {
            assert_eq!(
                *size, LABEL_FLOOR,
                "only the floor fits without the state row: {lines:?}"
            );
        }
        assert!(
            state_lines(&cell).is_empty(),
            "the state row is dropped, not drawn over the label: {:?}",
            state_lines(&cell)
        );
        assert_floor_respected(&cell, "state row yields");
    }

    /// module-ui-label-fit D6: the composed face gives the label the band beneath
    /// the shape without shrinking it. On a 9 mm P2B8 face the band is 17 pt, so
    /// the six-character label takes one fitted line (below 10 pt, where 36 pt of
    /// text cannot fit 34 pt) and the state circle keeps its full diameter.
    #[test]
    fn composed_face_keeps_its_shape_with_a_one_line_label_band() {
        let cell = composed_cell(9.0, 9.0, "Volume", CellShape::Round);
        let lines = label_lines(&cell);
        assert_eq!(lines.len(), 1, "one line in the 17 pt band: {lines:?}");
        assert_eq!(lines[0].0, "Volume", "the whole label: {lines:?}");
        let size = lines[0].1;
        assert_ladder_step(size, "composed one line");
        assert!(
            (LABEL_FLOOR..CELL_LABEL_SIZE).contains(&size),
            "a fitted step, never the fixed 10 pt: {lines:?}"
        );
        assert!(
            lines[0].2.y >= cell.rect.min.y + 0.5 * cell.rect.width(),
            "the label sits in the band beneath the shape: {:?}",
            lines[0].2
        );
        // The face was not shrunk for the label: measure it by the ring (the
        // stroked outline, `w/2 − 1` exactly), and by the fill staying inside it
        // (the LED fill legitimately reaches the cell's half-width at level 1).
        let rings: Vec<f32> = painted_shapes(&cell, false)
            .iter()
            .filter_map(|shape| match shape {
                egui::epaint::Shape::Circle(circle) if circle.stroke.width > 0.0 => {
                    Some(circle.radius)
                }
                _ => None,
            })
            .collect();
        let face = cell.rect.width().min(cell.rect.height()) / 2.0 - 1.0;
        assert_eq!(rings.len(), 1, "exactly one state ring: {rings:?}");
        assert!(
            (rings[0] - face).abs() < 0.01,
            "the ring keeps its full {face} pt radius for the label: {rings:?}"
        );
        let fills: Vec<f32> = painted_shapes(&cell, false)
            .iter()
            .filter_map(|shape| match shape {
                egui::epaint::Shape::Circle(circle)
                    if circle.stroke.width == 0.0 && circle.fill != egui::Color32::TRANSPARENT =>
                {
                    Some(circle.radius)
                }
                _ => None,
            })
            .collect();
        assert!(
            !fills.is_empty(),
            "the folded LED fills the face: {fills:?}"
        );
        for radius in fills {
            assert!(
                radius <= cell.rect.width().min(cell.rect.height()) / 2.0,
                "the LED fill stays inside the face: {radius} pt"
            );
        }
        assert_no_ellipsis(&cell, "composed one line");
        assert_floor_respected(&cell, "composed one line");
    }

    /// module-ui-label-fit D6 (spec "Label under a roomy shape"): the M4 touch
    /// plate is 12 x 18 mm, leaving a 45 pt band under its 45 pt face — two 10 pt
    /// lines. The shape is not displaced and the label never leaves the cell.
    #[test]
    fn composed_face_band_holds_two_label_lines_beneath_the_shape() {
        let cell = composed_cell(12.0, 18.0, "Master out", CellShape::Rect);
        let lines = label_lines(&cell);
        assert_eq!(lines.len(), 2, "two lines beneath the shape: {lines:?}");
        for (text, size, _) in &lines {
            assert_eq!(
                *size, LABEL_LADDER[0],
                "the top step holds both lines ({text:?}): {lines:?}"
            );
        }
        assert_eq!(
            label_text(&cell),
            "Master out",
            "the whole label: {lines:?}"
        );
        for (text, _, pos) in &lines {
            assert!(
                pos.y >= cell.rect.min.y + 0.5 * cell.rect.width(),
                "line {text:?} stays in the band beneath the shape: {pos:?}"
            );
        }
        let face = cell.rect.width().min(cell.rect.height()) - 2.0;
        let plate = painted_cell_rects(&cell, false)
            .into_iter()
            .find(|(rect, _, stroke, _)| *stroke > 0.0 && (rect.width() - face).abs() < 0.6);
        assert!(
            plate.is_some(),
            "the touch plate keeps its full {face} pt size: {:?}",
            painted_cell_rects(&cell, false)
        );
        assert_no_ellipsis(&cell, "composed two lines");
        assert_floor_respected(&cell, "composed two lines");
    }

    /// module-ui-label-fit D5 (spec "A fader label stacks"): the P8S8 fader
    /// (4.2 x 22 mm) narrows its track to 40 % of the cell (6.3 pt, under the old
    /// 10 pt clamp) and stacks the label one glyph per line in the column beside
    /// it — the old budget left 2.9 pt of text column and drew no label at all.
    #[test]
    fn p8s8_fader_narrows_its_track_and_stacks_one_glyph_per_line() {
        let cell = p8s8_fader_cell("Level");
        let width = cell.rect.width();
        let rects = painted_cell_rects(&cell, false);
        let (track, _, _, _) = rects
            .iter()
            .find(|(_, _, stroke, color)| {
                *stroke == 1.0 && *color == rgb(crate::theme::active().fader_slot)
            })
            .unwrap_or_else(|| panic!("fader slot outline painted: {rects:?}"));
        assert!(
            (track.width() - 0.4 * width).abs() < 0.6,
            "the track narrows to 40 % of the cell: {} pt vs {} pt",
            track.width(),
            0.4 * width
        );

        let runs: Vec<PaintedRun> = painted_runs(&cell)
            .into_iter()
            .filter(|run| {
                run.font.family == egui::FontFamily::Monospace && !run.text.trim().is_empty()
            })
            .collect();
        assert!(
            !runs.is_empty(),
            "the stacked label is painted beside the track: {runs:?}"
        );
        let glyphs: String = runs
            .iter()
            .flat_map(|run| run.text.chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        assert_eq!(glyphs, "Level", "every glyph, in reading order: {runs:?}");
        if runs.len() > 1 {
            assert_eq!(runs.len(), 5, "one text run per glyph: {runs:?}");
            for run in &runs {
                assert_eq!(
                    run.text.trim().chars().count(),
                    1,
                    "one glyph per line, not a horizontal run: {:?}",
                    run.text
                );
            }
            let ys: Vec<f32> = runs.iter().map(|run| run.pos.y).collect();
            let mut stacked = ys.clone();
            stacked.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(ys, stacked, "glyphs stack top to bottom: {ys:?}");
            stacked.dedup();
            assert_eq!(stacked.len(), 5, "each glyph on its own line: {ys:?}");
            let x0 = runs[0].pos.x;
            assert!(
                runs.iter().all(|run| (run.pos.x - x0).abs() < 0.5),
                "one column: {:?}",
                runs.iter().map(|run| run.pos.x).collect::<Vec<_>>()
            );
        } else {
            let lines: Vec<&str> = runs[0].text.split('\n').collect();
            assert_eq!(lines.len(), 5, "five stacked lines: {:?}", runs[0].text);
            assert!(
                lines.iter().all(|line| line.trim().chars().count() == 1),
                "one glyph per line: {:?}",
                runs[0].text
            );
        }
        for run in &runs {
            assert!(
                run.pos.x >= cell.rect.min.x + 0.4 * width - 0.5,
                "the stacked column sits beside the track, never over it: {:?}",
                run.pos
            );
            assert_ladder_step(run.font.size, "stacked fader");
            assert!(
                run.font.size >= LABEL_FLOOR,
                "the stack never drops below the floor: {:?}",
                run.font.size
            );
        }
        assert_floor_respected(&cell, "p8s8 fader");
    }

    /// module-ui-label-fit D2/D9 (spec "A label no step can hold stays
    /// reachable"): a 3 mm LED cell is 11.3 pt wide, so no step holds a
    /// four-character label even at the 6 pt floor (three characters per line,
    /// and two lines are 15 pt in an 11.3 pt cell). The cell draws its state and
    /// no label fragment.
    #[test]
    fn three_millimetre_led_cell_draws_no_in_cell_label() {
        let cell = sized_cell(3.0, 3.0, "\u{25CF}", "Gate", "ON");
        let lines = label_lines(&cell);
        assert!(
            lines.is_empty(),
            "no in-cell label where no step fits: {lines:?}"
        );
        for run in painted_runs(&cell) {
            assert!(
                !run.text.contains("Gate") && !run.text.contains('\u{2026}'),
                "no label fragment and no floor ellipsis in place of one: {:?}",
                run.text
            );
        }
        assert_floor_respected(&cell, "3 mm LED");
    }

    /// module-ui-label-fit D1 (the no-layout case): an empty label has no layout,
    /// so it paints no text. A composed cell has no glyph and no state row, so
    /// its cell is text-free; a compact cell keeps its state glyph and nothing
    /// else.
    #[test]
    fn empty_label_paints_no_text_run() {
        let mut composed = composed_cell(9.0, 9.0, "", CellShape::Round);
        composed.led_state = None;
        let runs = painted_runs(&composed);
        assert!(
            runs.is_empty(),
            "an empty label leaves no text run at all: {runs:?}"
        );

        let compact = sized_cell(9.0, 9.0, "\u{25CF}", "", "ON");
        let lines = label_lines(&compact);
        assert!(
            lines.is_empty(),
            "no label line for an empty label: {lines:?}"
        );
    }

    // -- module-ui-label-fit 1.5: the reveal outlives the fitted cell ----------
    //
    // A fitted cell can lose text in three ways — ellipsize at the floor, drop
    // the label entirely, or drop only the state row (D2/D4) — and the hover
    // reveal must still name the element and its state in every case, in every
    // label presentation, in the skeleton presentation, and through the pane's
    // status subject (module-ui "Element label reveal"). The scenario is one
    // P4B2 faceplate at true 1:1, whose cells are a roomy 12 mm pot, a 9 mm
    // button and a 3 mm LED: one written label per text fate, plus a roomy pot
    // whose label the cell draws whole.

    /// A label too long for the 12 mm pot at the ladder floor: the last line
    /// ellipsizes.
    const REVEAL_ELLIPSIZED_LABEL: &str =
        "Filter cutoff resonance envelope modulation master output stage";
    /// A label that fits the 9 mm button only after its state row yields (D4).
    const REVEAL_STATE_YIELDS_LABEL: &str = "Filter cutoff resonance";
    /// A label no ladder step holds in a 3 mm LED cell: the cell draws none.
    const REVEAL_DROPPED_LABEL: &str = "Gate";
    /// A short label the roomy pot draws whole, so the reveal is not a
    /// clipped-cell-only behaviour.
    const REVEAL_WHOLE_LABEL: &str = "Trim";

    /// `token -> written label` for every cell the reveal tests hover.
    const REVEAL_SCENARIO: [(&str, &str); 4] = [
        ("P5.1", REVEAL_ELLIPSIZED_LABEL),
        ("B5.1", REVEAL_STATE_YIELDS_LABEL),
        ("L5.1", REVEAL_DROPPED_LABEL),
        ("P5.2", REVEAL_WHOLE_LABEL),
    ];

    /// The reveal scenario's P4B2 faceplate with the written labels attached as
    /// preamble labels, at true 1:1 (the zoom the layout fates above are computed
    /// for), in `mode`.
    fn reveal_app(mode: LabelPresentation) -> App {
        let mut app = app_with_labels("fixtures/faceplate_rotary_stack.ini", &REVEAL_SCENARIO);
        app.label_presentation = mode;
        app.physical_zoom = 1.0;
        app.physical_offset = (0.0, 0.0);
        app
    }

    fn reveal_modes() -> [LabelPresentation; 3] {
        [
            LabelPresentation::Off,
            LabelPresentation::Auto,
            LabelPresentation::On,
        ]
    }

    /// Hover `token` and return the spec built for it plus the cell it draws.
    /// The hover reveal and the cell's own layout come from the same build, so a
    /// test cannot compare a callout against a differently-built cell.
    fn reveal_spec(app: &mut App, token: &str) -> (PhysicalSpec, CellSpec) {
        app.hovered_component = Some(hw_index(app, token));
        let spec = physical_spec(app, perf_pane()).expect("spec");
        let cell = cell_for_token(app, &spec, token).clone();
        (spec, cell)
    }

    /// D2/D4: the roomy pot's long label ellipsizes in its cell, yet the hover
    /// callout names all of it in every label presentation and in the skeleton.
    #[test]
    fn reveal_callout_names_the_whole_label_where_the_cell_ellipsized() {
        for mode in reveal_modes() {
            for skeleton in [false, true] {
                let mut app = reveal_app(mode);
                app.physical_show_skeleton = skeleton;
                let (spec, cell) = reveal_spec(&mut app, "P5.1");
                let layout = cell
                    .label_layout
                    .as_ref()
                    .unwrap_or_else(|| panic!("the pot keeps a floor layout ({mode:?})"));
                assert!(
                    layout.lines.iter().any(|line| line.contains('\u{2026}')),
                    "the long pot label ellipsizes in its cell ({mode:?}, skeleton={skeleton}): {layout:?}"
                );
                assert_ne!(
                    label_text(&cell),
                    REVEAL_ELLIPSIZED_LABEL,
                    "the cell paints the ellipsized form, not the whole label ({mode:?}, skeleton={skeleton})"
                );
                let hover = spec.hover.as_ref().unwrap_or_else(|| {
                    panic!("a callout for the ellipsized cell ({mode:?}, skeleton={skeleton})")
                });
                assert_eq!(
                    hover.label, REVEAL_ELLIPSIZED_LABEL,
                    "the callout names the whole label ({mode:?}, skeleton={skeleton})"
                );
                // The scenario's three cells all carry a state string, so the
                // empty-state branch is not the case asserted here.
                assert!(
                    !cell.state_text.is_empty(),
                    "the pot's cell carries a state ({mode:?})"
                );
                assert_eq!(
                    hover.state_text, cell.state_text,
                    "the callout carries the cell's state ({mode:?}, skeleton={skeleton})"
                );
                assert!(
                    painted_texts(&spec).iter().any(|(text, font, _)| {
                        text == REVEAL_ELLIPSIZED_LABEL
                            && *font == egui::FontId::monospace(CALLOUT_LABEL_SIZE)
                    }),
                    "the reveal paints the full label at {CALLOUT_LABEL_SIZE} pt ({mode:?}, skeleton={skeleton})"
                );
            }
        }
    }

    /// D2/D9: the 3 mm LED cell holds no ladder step's worth of its label, so the
    /// cell draws none — and the reveal still names it and its state.
    #[test]
    fn reveal_callout_names_the_label_where_the_cell_dropped_it_entirely() {
        for mode in reveal_modes() {
            for skeleton in [false, true] {
                let mut app = reveal_app(mode);
                app.physical_show_skeleton = skeleton;
                let (spec, cell) = reveal_spec(&mut app, "L5.1");
                assert!(
                    cell.label_layout.is_none(),
                    "no ladder step holds the LED label ({mode:?}, skeleton={skeleton}): {:?}",
                    cell.label_layout
                );
                assert!(
                    label_text(&cell).is_empty(),
                    "the cell paints no label text ({mode:?}, skeleton={skeleton}): {:?}",
                    label_text(&cell)
                );
                let hover = spec.hover.as_ref().unwrap_or_else(|| {
                    panic!("a callout for the dropped-label cell ({mode:?}, skeleton={skeleton})")
                });
                assert_eq!(
                    hover.label, REVEAL_DROPPED_LABEL,
                    "the callout names the label the cell dropped ({mode:?}, skeleton={skeleton})"
                );
                assert!(
                    !cell.state_text.is_empty(),
                    "the LED's cell carries a state ({mode:?})"
                );
                assert_eq!(
                    hover.state_text, cell.state_text,
                    "the callout carries the cell's state ({mode:?}, skeleton={skeleton})"
                );
            }
        }
    }

    /// D4: the 9 mm button fits its label only by giving up the state row, so the
    /// cell paints no state — and the callout still reports the state value.
    #[test]
    fn reveal_callout_carries_the_state_the_cell_dropped_from_its_row() {
        for mode in reveal_modes() {
            for skeleton in [false, true] {
                let mut app = reveal_app(mode);
                app.physical_show_skeleton = skeleton;
                let (spec, cell) = reveal_spec(&mut app, "B5.1");
                let layout = cell.label_layout.as_ref().unwrap_or_else(|| {
                    panic!("the button still draws its label ({mode:?}, skeleton={skeleton})")
                });
                assert!(
                    !layout.state_row,
                    "the label took the cell and the state row yielded ({mode:?}, skeleton={skeleton}): {layout:?}"
                );
                assert!(
                    state_lines(&cell).is_empty(),
                    "the cell paints no state row ({mode:?}, skeleton={skeleton}): {:?}",
                    state_lines(&cell)
                );
                assert_eq!(
                    label_text(&cell).replace(' ', ""),
                    "Filtercutoffresonance",
                    "the cell still draws the whole label ({mode:?}, skeleton={skeleton})"
                );
                let hover = spec.hover.as_ref().unwrap_or_else(|| {
                    panic!("a callout for the state-yielding cell ({mode:?}, skeleton={skeleton})")
                });
                assert_eq!(
                    hover.label, REVEAL_STATE_YIELDS_LABEL,
                    "the callout names the whole label ({mode:?}, skeleton={skeleton})"
                );
                assert!(
                    !cell.state_text.is_empty(),
                    "the button's cell carries a state ({mode:?})"
                );
                assert_eq!(
                    hover.state_text, cell.state_text,
                    "the callout carries the state the cell's row could not ({mode:?}, skeleton={skeleton})"
                );
            }
        }
    }

    /// module-ui "Element label reveal": the reveal is not a clipped-cell-only
    /// behaviour — a cell that draws its whole label still reveals it on hover.
    #[test]
    fn reveal_also_fires_for_a_label_the_cell_draws_whole() {
        for mode in reveal_modes() {
            let mut app = reveal_app(mode);
            let (spec, cell) = reveal_spec(&mut app, "P5.2");
            let layout = cell
                .label_layout
                .as_ref()
                .unwrap_or_else(|| panic!("the short pot label fits its cell ({mode:?})"));
            assert_eq!(
                layout.lines,
                vec![REVEAL_WHOLE_LABEL.to_string()],
                "the whole label is drawn in the cell ({mode:?})"
            );
            assert_no_ellipsis(&cell, "whole label");
            assert_eq!(
                label_text(&cell),
                REVEAL_WHOLE_LABEL,
                "the cell already names the element ({mode:?})"
            );
            let hover = spec
                .hover
                .as_ref()
                .unwrap_or_else(|| panic!("a callout for the whole-label cell ({mode:?})"));
            assert_eq!(
                hover.label, REVEAL_WHOLE_LABEL,
                "the reveal fires for a label the cell draws whole ({mode:?})"
            );
            assert_eq!(
                hover.state_text, cell.state_text,
                "the callout carries the cell's state ({mode:?})"
            );
        }
    }

    /// The skeleton presentation draws no fitted cell text, but the reveal still
    /// names the element — and its callout is the same one the full view draws.
    #[test]
    fn reveal_is_identical_in_the_skeleton_presentation() {
        for mode in reveal_modes() {
            let mut app = reveal_app(mode);
            app.physical_show_skeleton = false;
            let (full, _) = reveal_spec(&mut app, "P5.1");
            let full_hover = full.hover.clone().expect("full reveal");

            app.physical_show_skeleton = true;
            let (skeleton, _) = reveal_spec(&mut app, "P5.1");
            let skeleton_hover = skeleton.hover.clone().expect("skeleton reveal");

            assert_eq!(
                skeleton_hover.label, REVEAL_ELLIPSIZED_LABEL,
                "the skeleton reveal still names the element ({mode:?})"
            );
            assert_eq!(
                skeleton_hover.label, full_hover.label,
                "the skeleton reveals the same label as the full view ({mode:?})"
            );
            assert_eq!(
                skeleton_hover.state_text, full_hover.state_text,
                "the skeleton reveals the same state as the full view ({mode:?})"
            );
            assert_eq!(
                skeleton_hover.label_rect, full_hover.label_rect,
                "the skeleton callout is placed identically ({mode:?})"
            );
            assert_eq!(
                skeleton_hover.anchor, full_hover.anchor,
                "the skeleton callout anchors identically ({mode:?})"
            );
        }
    }

    /// No element under the pointer: no callout, in every mode and in the
    /// skeleton presentation.
    #[test]
    fn reveal_is_absent_with_no_hovered_component() {
        for mode in reveal_modes() {
            for skeleton in [false, true] {
                let mut app = reveal_app(mode);
                app.physical_show_skeleton = skeleton;
                app.hovered_component = None;
                let spec = physical_spec(&app, perf_pane()).expect("spec");
                assert!(
                    spec.hover.is_none(),
                    "no callout without a hovered element ({mode:?}, skeleton={skeleton})"
                );
            }
        }
    }

    /// The pane's status subject names the same element and state as the callout:
    /// for every cell whose layout lost text, and in every label presentation.
    #[test]
    fn reveal_status_subject_names_the_full_label_and_state_in_every_mode() {
        for mode in reveal_modes() {
            let mut app = reveal_app(mode);
            for (token, label) in REVEAL_SCENARIO {
                let (spec, cell) = reveal_spec(&mut app, token);
                let hover = spec
                    .hover
                    .as_ref()
                    .unwrap_or_else(|| panic!("a callout for {token} ({mode:?})"));
                assert!(
                    !cell.state_text.is_empty(),
                    "the scenario's {token} cell carries a state ({mode:?})"
                );
                let subject = crate::gui::module_ui_status_subject(&app)
                    .unwrap_or_else(|| panic!("a status subject for {token} ({mode:?})"));
                assert_eq!(
                    subject,
                    format!("{label}: {}", cell.state_text),
                    "the status subject names the full label and state ({token}, {mode:?})"
                );
                assert!(
                    subject.starts_with(label),
                    "the status subject names the element, not a cell fragment ({token}, {mode:?}): {subject:?}"
                );
                assert_eq!(
                    subject,
                    format!("{}: {}", hover.label, hover.state_text),
                    "the status subject and the callout agree ({token}, {mode:?})"
                );
            }
        }
    }
}
