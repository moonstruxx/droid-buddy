//! Pure render-metrics extraction for the render-outlier detector (design D1-D5).
//!
//! This module computes a deterministic feature vector for a `(Patch, width,
//! theme)` triple — **without rendering a frame** — so an offline decision
//! table (task 2.x) can later predict when a patch's rendering degrades at the
//! user's terminal width or palette: boxed cells falling back to unboxed
//! two-line cells, a hidden source sidebar / minimap, or failing mono contrast.
//!
//! ## Contract (design D2): the extractor mirrors the renderer, never guesses
//!
//! Every feature is derived from the renderer's own layout constants
//! (`COMPONENT_WIDTH`, `COMPONENT_HEIGHT`, `BOX_MIN_WIDTH`, `MINIMAP_WIDTH` in
//! `src/ui.rs` — imported here, never re-declared) and its layout decisions.
//! If the renderer's layout changes, this module's mirrored arithmetic must
//! change with it; the in-module tests pin the mirror to the renderer's
//! behavior (see the regression tests for sidebar/minimap thresholds, which
//! lock the same constants at runtime).
//!
//! ## Layout model
//!
//! Features model the **embedded source view** (panels | source, 60/40 split),
//! **portrait** orientation, **scale 1.0** — the layout where the panel grid,
//! the source sidebar, and the minimap coexist, so every degradation channel
//! is observable from one feature vector. This mirrors `render_embedded_main`
//! (`src/ui.rs`): panels pane at `Percentage(60)`, source pane at
//! `Percentage(40)`, sidebar visible at source-pane width ≥ 40, minimap
//! subject to the same ≥ 40 floor plus the ≥ 20 remaining-content guard.
//! Height is assumed sufficient (the extractor models width-driven
//! degradation; `should_show_minimap`'s height check is documented but not
//! modeled).
//!
//! `min_width` is the patch's **landscape natural width** at scale 1.0 — the
//! width at which every panel shows its full content side by side
//! (flat panel: visible components × `COMPONENT_WIDTH` + 2 borders; subdivided
//! panel: widest module × `COMPONENT_WIDTH` + 4). It is the honest "this patch
//! wants ≥ N columns" number surfaced by the status hint, and `overflow_cols`
//! is the monotone shortfall `max(0, min_width − width)` against it.
//!
//! `min_contrast` is the minimum WCAG contrast ratio between co-occurring
//! panel-surface tokens (the component-kind colors actually used by this
//! patch, plus `text`/`muted` which always render, plus the four shift colors
//! when the patch uses shift groups) and the assumed terminal background.
//! Colors are resolved to the palettes' intended ANSI-16 sRGB values; a
//! `Color::Reset` token (terminal palette) is unresolvable and skipped, so a
//! fully-reset theme yields `None`.

use std::collections::{HashMap, HashSet};

use ratatui::style::Color;

use crate::patch::{ComponentKind, HwComponent, Patch};
use crate::theme::Theme;
use crate::ui::{BOX_MIN_WIDTH, COMPONENT_WIDTH, MINIMAP_WIDTH};

/// Panels share of the embedded main-area split (`render_embedded_main`).
pub(crate) const PANELS_PERCENT: u16 = 60;
/// Source pane share of the embedded main-area split.
pub(crate) const SOURCE_PERCENT: u16 = 40;
/// Source-pane width floor below which the sidebar and minimap are hidden
/// (`render_source_pane`: `show_sidebar = area.width >= 40`).
pub(crate) const SIDEBAR_FLOOR_COLS: u16 = 40;
/// Minimum columns the source content must keep after sidebar + minimap
/// before either is hidden (`render_source_pane` `content_min`).
pub(crate) const CONTENT_MIN_COLS: u16 = 20;
/// Sidebar width = `source_w / SIDEBAR_DIVISOR` clamped to
/// `[SIDEBAR_MIN_WIDTH, source_w − 20]` (`render_source_pane`).
pub(crate) const SIDEBAR_DIVISOR: u16 = 5;
pub(crate) const SIDEBAR_MIN_WIDTH: u16 = 20;
/// Assumed terminal background for contrast math: the classic palette's dark
/// baseline. Documented, not queried — the extractor cannot know the user's
/// terminal background, so it pins the design baseline (D4 determinism).
pub(crate) const DEFAULT_BG_RGB: (u8, u8, u8) = (0, 0, 0);

/// Deterministic feature vector for one `(patch, width, theme)` triple.
///
/// Pure: `extract` reads only its arguments, so two calls with the same
/// inputs produce byte-identical output (the task's determinism requirement).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFeatures {
    /// Terminal width the features were extracted for.
    pub width: u16,
    /// Visible component cells the renderer actually draws (folded LEDs — LEDs
    /// referenced by another component and matching a standalone Led — are
    /// skipped as grid cells, mirroring `render_patch_grouped`).
    pub components: usize,
    /// Controller panels in first-appearance order (`panel_order`).
    pub panels: usize,
    /// Module sub-blocks: 1 per flat panel, one per circuit instance for
    /// subdivided panels (CV I/O never subdivides).
    pub modules: usize,
    /// Landscape natural width at scale 1.0: the width at which every panel
    /// renders its full content side by side without wrapping (see module
    /// docs). Monotone patch property, theme-independent.
    pub min_width: u16,
    /// `max(0, min_width − width)`: columns the current width falls short of
    /// the patch's native fit; 0 when the width fits.
    pub overflow_cols: u16,
    /// Fraction of visible component cells that degrade to the unboxed
    /// two-line fallback (`cell_w < BOX_MIN_WIDTH`, mirroring
    /// `render_component_grid` + `render_component`'s boxed guard). `0.0`
    /// when the patch has no visible components.
    pub fallback_rate: f32,
    /// True when the source sidebar would be hidden at this width
    /// (source pane < 40 cols, or the patch has no `[sections]`, or the
    /// remaining-content guard fires).
    pub sidebar_hidden: bool,
    /// True when the minimap would be hidden at this width (source pane < 40
    /// cols, terminal < 80 cols, no patch content, or the ≥ 20 remaining
    /// columns guard fires).
    pub minimap_hidden: bool,
    /// Minimum WCAG contrast ratio (1.0..=21.0) between co-occurring
    /// panel-surface tokens and the assumed background; `None` when no
    /// co-occurring token pair resolves to RGB (fully `Color::Reset` themes).
    pub min_contrast: Option<f32>,
}

impl RenderFeatures {
    /// Extract the feature vector for `patch` at `width` under `theme`.
    ///
    /// Pure and deterministic; never renders a frame, never touches the app
    /// or terminal state.
    pub fn extract(patch: &Patch, width: u16, theme: &Theme) -> Self {
        let models = panel_models(patch);
        let components: usize = models.iter().map(|m| m.visible.len()).sum();
        let panels = models.len();
        let modules: usize = models.iter().map(|m| m.groups.len()).sum();

        let min_width: u16 = models.iter().map(|m| m.natural_width()).sum();
        let overflow_cols = min_width.saturating_sub(width);

        let fallback_rate = fallback_rate(&models, width);

        let (sidebar_hidden, minimap_hidden) = source_surface_decisions(patch, width);

        let min_contrast = min_contrast(patch, theme);

        Self {
            width,
            components,
            panels,
            modules,
            min_width,
            overflow_cols,
            fallback_rate,
            sidebar_hidden,
            minimap_hidden,
            min_contrast,
        }
    }
}

// ── learned decision table + scorer (task 2.2, design D1/D5) ─────────────

/// Embedded distilled decision table (tools/render_artifact.txt, task 2.1).
/// The schema-drift check (design D1) lives in `parse_artifact`: a mismatch
/// between the fitter and this extractor breaks a test, not a user session.
const RENDER_ARTIFACT_JSON: &str = include_str!("../tools/render_artifact.txt");
const ARTIFACT_VERSION: u32 = 1;

/// Feature names the artifact must declare, in exact order — the extractor's
/// feature set (schema-drift check against `tools/build_rendermetrics.py`).
pub(crate) const FEATURE_NAMES: [&str; 9] = [
    "components",
    "panels",
    "modules",
    "min_width",
    "overflow_cols",
    "fallback_rate",
    "sidebar_hidden",
    "minimap_hidden",
    "min_contrast",
];

/// Sentinel for an unresolvable `min_contrast` (terminal theme, all tokens
/// `Color::Reset`): above any real contrast value, so no learned band ever
/// matches it — the terminal theme owns its colors and is never flagged.
const CONTRAST_NA_SENTINEL: f64 = 99.0;

/// Which degradation channel produced a render outlier.
#[derive(Debug, Clone, PartialEq)]
pub enum DegradeChannel {
    /// Native-fit clipping: `overflow_cols > 0` (matched the table's
    /// `overflow_cols` band, or the D5 fallback). Structurally impossible at
    /// or above the patch's native-fit width (overflow is 0 there).
    Overflow,
    /// Palette contrast failure: `min_contrast < 4.5` (the mono shift4=Black
    /// failure the detector exists for). Width-independent.
    Contrast,
    /// Boxed→unboxed cell fallback (`fallback_rate > 0`), or any other
    /// table match without a dedicated channel.
    Fallback,
}

/// A predicted render degradation, advisory only (design D5): it never gates
/// loading, never blocks rendering, never intercepts input.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOutlier {
    /// The channel that fired.
    pub channel: DegradeChannel,
    /// Columns the patch wants at scale 1.0 (its native-fit `min_width`) —
    /// the "use ≥ M cols" recommendation for the status hint.
    pub recommended_width: u16,
    /// The minimum co-occurring contrast that fired (contrast channel only;
    /// `None` for themes whose tokens are unresolvable).
    pub min_contrast: Option<f32>,
}

/// The distilled artifact: bounded per-feature bands (design D1).
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct RenderArtifact {
    version: u32,
    feature_names: Vec<String>,
    degraded_bands: Vec<RenderBand>,
    baseline_min_width_factor: f64,
    seed: u32,
}

/// One band over one feature: a row matches when its feature value lies in
/// `[min, max]`. `degraded: true` flags, `degraded: false` passes; bands are
/// evaluated top-down, first match wins. The fitter emits only flag bands
/// (pass bands would weaken the baseline's guarantees, design D5).
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct RenderBand {
    feature: String,
    min: f64,
    max: f64,
    degraded: bool,
}

/// Parse and schema-check the embedded artifact. Errors on drift: wrong
/// version, feature names not matching the extractor's set, a band over an
/// unknown feature, an inverted band, or a baseline factor ≠ 1.0 (the
/// fallback rule implements `width < min_width × factor`, and only factor
/// 1.0 — the committed artifact's value — is supported).
pub(crate) fn parse_artifact(json: &str) -> Result<RenderArtifact, String> {
    let artifact: RenderArtifact =
        serde_json::from_str(json).map_err(|e| format!("render artifact parse: {e}"))?;
    if artifact.version != ARTIFACT_VERSION {
        return Err(format!(
            "render artifact version {} != extractor version {ARTIFACT_VERSION}",
            artifact.version
        ));
    }
    if artifact.feature_names != FEATURE_NAMES {
        return Err(format!(
            "render artifact feature_names {:?} != extractor {:?}",
            artifact.feature_names, FEATURE_NAMES
        ));
    }
    for band in &artifact.degraded_bands {
        if !artifact.feature_names.iter().any(|n| n == &band.feature) {
            return Err(format!(
                "render artifact band over unknown feature {:?}",
                band.feature
            ));
        }
        if band.min > band.max {
            return Err(format!("render artifact band {:?} inverted", band.feature));
        }
    }
    if artifact.baseline_min_width_factor != 1.0 {
        return Err(format!(
            "render artifact baseline_min_width_factor {} != 1.0 (fallback semantics)",
            artifact.baseline_min_width_factor
        ));
    }
    if artifact.seed != 42 {
        return Err(format!(
            "render artifact seed {} != 42 (corpus determinism seed)",
            artifact.seed
        ));
    }
    Ok(artifact)
}

/// One feature's value as `f64` for band matching. `min_contrast` uses the
/// NA sentinel when the theme's tokens are unresolvable. An unknown feature
/// is unreachable (parse_artifact validates band names) and yields NaN so
/// no band can match it.
fn feature_value(features: &RenderFeatures, name: &str) -> f64 {
    match name {
        "components" => features.components as f64,
        "panels" => features.panels as f64,
        "modules" => features.modules as f64,
        "min_width" => features.min_width as f64,
        "overflow_cols" => features.overflow_cols as f64,
        "fallback_rate" => features.fallback_rate as f64,
        "sidebar_hidden" => u8::from(features.sidebar_hidden) as f64,
        "minimap_hidden" => u8::from(features.minimap_hidden) as f64,
        "min_contrast" => features
            .min_contrast
            .map(f64::from)
            .unwrap_or(CONTRAST_NA_SENTINEL),
        _ => f64::NAN,
    }
}

fn outlier_for(band: &RenderBand, features: &RenderFeatures) -> RenderOutlier {
    let channel = match band.feature.as_str() {
        "overflow_cols" => DegradeChannel::Overflow,
        "min_contrast" => DegradeChannel::Contrast,
        _ => DegradeChannel::Fallback,
    };
    RenderOutlier {
        channel,
        recommended_width: features.min_width,
        min_contrast: features.min_contrast,
    }
}

/// Score features against an explicit band set (testable core).
///
/// Invariant guards (design D5), enforced structurally:
///   1. Native fit never flagged by the width channel — `overflow_cols == 0`
///      exactly when `width >= min_width`, so neither the overflow band nor
///      the fallback can fire at/above native fit. Contrast and fallback-rate
///      channels are palette-dependent and orthogonal (the mono failure).
///   2. Baseline clean never flagged — the fallback only flags when the
///      baseline (native-fit rule) itself flags.
///   3. Miss → heuristic fallback — a row matching no band falls back to the
///      baseline rule below.
pub(crate) fn score_with_bands(
    features: &RenderFeatures,
    bands: &[RenderBand],
) -> Option<RenderOutlier> {
    for band in bands {
        let value = feature_value(features, &band.feature);
        if band.min <= value && value <= band.max {
            return if band.degraded {
                Some(outlier_for(band, features))
            } else {
                None
            };
        }
    }
    // D5 miss → heuristic fallback: the native-fit rule (width < min_width
    // × factor; factor validated == 1.0, so this is overflow_cols > 0).
    if features.overflow_cols > 0 {
        Some(RenderOutlier {
            channel: DegradeChannel::Fallback,
            recommended_width: features.min_width,
            min_contrast: features.min_contrast,
        })
    } else {
        None
    }
}

/// Score a feature vector against the embedded learned table.
/// `Err` means schema drift (design D1: breaks a test, not a user session);
/// the caller decides how to surface it (task 3.1: no hint).
pub fn score_render(features: &RenderFeatures) -> Result<Option<RenderOutlier>, String> {
    let artifact = parse_artifact(RENDER_ARTIFACT_JSON)?;
    Ok(score_with_bands(features, &artifact.degraded_bands))
}

/// One controller panel as the renderer groups it (`render_patch_grouped`):
/// visible components (folded LEDs removed) split into module groups.
struct PanelModel<'a> {
    visible: Vec<&'a HwComponent>,
    /// Module sub-groups in first-appearance order; a flat panel has exactly
    /// one group (`groups.len() == 1` == not subdivided).
    groups: Vec<Vec<&'a HwComponent>>,
}

impl<'a> PanelModel<'a> {
    /// Subdivided = more than one module group (the renderer draws module
    /// sub-borders only when a panel mixes multiple circuit instances).
    fn subdivided(&self) -> bool {
        self.groups.len() > 1
    }

    /// Landscape natural width at scale 1.0, mirroring `panel_grid_size` with
    /// ample inner width: flat panels show every visible component on one row
    /// (`n × COMPONENT_WIDTH + 2`); subdivided panels show the widest module
    /// on one row (`widest × COMPONENT_WIDTH + 4`).
    fn natural_width(&self) -> u16 {
        if self.subdivided() {
            let widest = self.groups.iter().map(|g| g.len()).max().unwrap_or(0);
            (widest as u16) * COMPONENT_WIDTH + 4
        } else {
            (self.visible.len() as u16) * COMPONENT_WIDTH + 2
        }
    }
}

/// Group the patch's components into panel models, mirroring
/// `render_patch_grouped`'s panel order, folded-LED filtering, and
/// per-instance module grouping (CV I/O never subdivides).
fn panel_models(patch: &Patch) -> Vec<PanelModel<'_>> {
    // Folded LEDs: referenced by another component's `led` field AND matching
    // a standalone Led component — skipped as grid cells (design: they render
    // as boxes inside their owner, not as separate cells).
    let folded_led_ids: HashSet<&str> = patch
        .hw_components
        .iter()
        .filter(|c| c.led.is_some())
        .filter_map(|c| c.led.as_deref())
        .filter(|led_id| {
            patch
                .hw_components
                .iter()
                .any(|c| c.id == *led_id && c.kind == ComponentKind::Led)
        })
        .collect();

    // Panels in first-appearance order.
    let mut order: Vec<&str> = Vec::new();
    let mut by_panel: HashMap<&str, Vec<&HwComponent>> = HashMap::new();
    for comp in &patch.hw_components {
        by_panel
            .entry(comp.controller.as_str())
            .or_insert_with(|| {
                order.push(comp.controller.as_str());
                Vec::new()
            })
            .push(comp);
    }

    order
        .into_iter()
        .map(|name| {
            let visible: Vec<&HwComponent> = by_panel[name]
                .iter()
                .copied()
                .filter(|c| {
                    !(c.kind == ComponentKind::Led && folded_led_ids.contains(c.id.as_str()))
                })
                .collect();

            let groups = if name == "CV I/O" {
                vec![visible.clone()]
            } else {
                let mut instance_order: Vec<u32> = Vec::new();
                let mut by_instance: HashMap<u32, Vec<&HwComponent>> = HashMap::new();
                for comp in &visible {
                    let key = comp.module_instance().unwrap_or(0);
                    by_instance
                        .entry(key)
                        .or_insert_with(|| {
                            instance_order.push(key);
                            Vec::new()
                        })
                        .push(*comp);
                }
                if instance_order.len() <= 1 {
                    vec![visible.clone()]
                } else {
                    instance_order
                        .into_iter()
                        .map(|k| by_instance.remove(&k).unwrap())
                        .collect()
                }
            };

            PanelModel { visible, groups }
        })
        .collect()
}

/// Fraction of visible cells falling back to unboxed rendering, mirroring the
/// embedded portrait layout: panels pane at `PANELS_PERCENT` of the width,
/// minus the Panels block border (2), minus the panel border (2) for flat
/// grids or the module border (4) for subdivided ones. A cell is unboxed when
/// its real width `cell_w = min(COMPONENT_WIDTH, grid_w)` is below
/// `BOX_MIN_WIDTH` (the height guard `scaled_h >= COMPONENT_HEIGHT` always
/// holds at scale 1.0: 3 ≥ 3).
fn fallback_rate(models: &[PanelModel<'_>], width: u16) -> f32 {
    let panels_area_w = width.saturating_mul(PANELS_PERCENT) / 100;
    let panels_pane_inner = panels_area_w.saturating_sub(2);

    let mut unboxed = 0usize;
    let mut total = 0usize;
    for model in models {
        let grid_w = if model.subdivided() {
            panels_pane_inner.saturating_sub(4)
        } else {
            panels_pane_inner.saturating_sub(2)
        };
        let cell_w = COMPONENT_WIDTH.min(grid_w.max(1));
        if cell_w < BOX_MIN_WIDTH {
            unboxed += model.visible.len();
        }
        total += model.visible.len();
    }
    if total == 0 {
        0.0
    } else {
        unboxed as f32 / total as f32
    }
}

/// Sidebar and minimap visibility decisions, mirroring `render_source_pane`'s
/// cascade exactly (order matters): sidebar floor → sidebar sizing →
/// minimap floor → remaining-content guard → sidebar squeeze guard.
fn source_surface_decisions(patch: &Patch, width: u16) -> (bool, bool) {
    let source_w = width.saturating_mul(SOURCE_PERCENT) / 100;
    let has_sections = !patch.sections.is_empty();
    let has_content = has_sections || !patch.raw_lines.is_empty();

    let mut show_sidebar = source_w >= SIDEBAR_FLOOR_COLS && has_sections;
    let mut sidebar_w = if show_sidebar {
        (source_w / SIDEBAR_DIVISOR)
            .max(SIDEBAR_MIN_WIDTH)
            .min(source_w.saturating_sub(20))
    } else {
        0
    };
    if sidebar_w >= source_w {
        show_sidebar = false;
        sidebar_w = 0;
    }

    // should_show_minimap: patch present (given), content present, terminal
    // ≥ 80 cols, source pane ≥ 40 cols, pane height ≥ 10 (height assumed).
    let mut show_minimap = width >= 80 && source_w >= SIDEBAR_FLOOR_COLS && has_content;
    let mut remaining = source_w
        .saturating_sub(sidebar_w)
        .saturating_sub(if show_minimap { MINIMAP_WIDTH } else { 0 });
    if show_minimap && remaining < CONTENT_MIN_COLS {
        show_minimap = false;
        remaining = source_w.saturating_sub(sidebar_w);
    }
    if show_sidebar && remaining < CONTENT_MIN_COLS {
        show_sidebar = false;
        // Renderer re-derives remaining from the (possibly zeroed) minimap
        // width, then guards the minimap on the bare source width.
        remaining = source_w.saturating_sub(if show_minimap { MINIMAP_WIDTH } else { 0 });
        if remaining < CONTENT_MIN_COLS && show_minimap {
            show_minimap = false;
        }
    }

    (!show_sidebar, !show_minimap)
}

/// Minimum WCAG 2.x contrast ratio between co-occurring panel-surface tokens
/// and the assumed terminal background (`DEFAULT_BG_RGB`).
///
/// Co-occurring set (documented): the tokens a component grid row actually
/// renders — `text` (labels/state), `muted` (panel borders/titles), and the
/// per-kind colors mapped from the patch's components
/// (`Button→button`, `Switch→switch`, `Knob`/`Encoder→knob`, `Led→led`,
/// `CvIn→cv_in`, `CvOut→cv_out`), plus `shift1..shift4` when the patch uses
/// shift groups. Tokens that resolve to `Color::Reset` are skipped; if no
/// pair resolves, the result is `None` (the terminal theme owns its colors).
fn min_contrast(patch: &Patch, theme: &Theme) -> Option<f32> {
    let mut tokens: Vec<Color> = vec![theme.text, theme.muted];
    for comp in &patch.hw_components {
        let token = match comp.kind {
            ComponentKind::Button => theme.button,
            ComponentKind::Switch => theme.switch,
            ComponentKind::Knob | ComponentKind::Encoder => theme.knob,
            ComponentKind::Led => theme.led,
            ComponentKind::CvIn => theme.cv_in,
            ComponentKind::CvOut => theme.cv_out,
        };
        tokens.push(token);
    }
    if patch.hw_components.iter().any(|c| c.shift_group.is_some()) {
        tokens.extend([theme.shift1, theme.shift2, theme.shift3, theme.shift4]);
    }

    let bg = DEFAULT_BG_RGB;
    let bg_lum = rgb_luminance(bg.0, bg.1, bg.2);
    tokens
        .into_iter()
        .filter_map(|fg| {
            let (r, g, b) = color_to_rgb(fg)?;
            Some(contrast_ratio(rgb_luminance(r, g, b), bg_lum))
        })
        .reduce(f32::min)
}

/// Resolve a theme color to sRGB using the palettes' intended ANSI-16 values
/// (documented in the module docs). `Color::Rgb` passes through; `Reset` and
/// `Indexed` (index beyond the 16 named colors) are unresolvable.
fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((255, 0, 0)),
        Color::Green => Some((0, 255, 0)),
        Color::Yellow => Some((255, 255, 0)),
        Color::Blue => Some((0, 0, 255)),
        Color::Magenta => Some((255, 0, 255)),
        Color::Cyan => Some((0, 255, 255)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::Gray => Some((192, 192, 192)),
        Color::White => Some((255, 255, 255)),
        _ => None,
    }
}

/// WCAG 2.x relative luminance of an sRGB channel value.
fn channel_luminance(channel: u8) -> f32 {
    let s = channel as f32 / 255.0;
    if s <= 0.039_28 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.x relative luminance of an sRGB color.
fn rgb_luminance(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * channel_luminance(r) + 0.7152 * channel_luminance(g) + 0.0722 * channel_luminance(b)
}

/// WCAG 2.x contrast ratio between two relative luminances (1.0..=21.0).
fn contrast_ratio(a: f32, b: f32) -> f32 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::theme;

    fn load_fixture(name: &str) -> Patch {
        Patch::from_ini_file(Path::new(&format!("fixtures/{name}.ini")))
            .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    fn classic() -> &'static Theme {
        theme::resolve("classic")
    }

    fn mono() -> &'static Theme {
        theme::resolve("mono")
    }

    fn terminal() -> &'static Theme {
        theme::resolve("terminal")
    }

    /// A tiny hand-built patch with one shift-grouped component, to exercise
    /// the shift-token contrast channel without depending on a fixture.
    fn shift_patch() -> Patch {
        let mut patch = Patch::from_ini_str(
            "[button]\n    button = B1.1\n    led = L1.1\n",
            String::from("shift"),
        )
        .expect("shift patch parses");
        for comp in &mut patch.hw_components {
            comp.shift_group = Some(crate::patch::ShiftGroup::Group1);
        }
        patch
    }

    // ── counts ──────────────────────────────────────────────────────────────

    #[test]
    fn arpeggio_counts_match_renderer() {
        let patch = load_fixture("arpeggio1");
        // Independent ground truth: the gallery ANSI evidence renders P2B8 as
        // 10 cells (8 buttons + speed + pattern, 8 LEDs folded into buttons)
        // and CV I/O as 4 jacks → 14 visible, 2 panels, 2 modules.
        let f = RenderFeatures::extract(&patch, 120, classic());
        assert_eq!(f.components, 14);
        assert_eq!(f.panels, 2);
        assert_eq!(f.modules, 2);
    }

    #[test]
    fn multi_module_counts_one_panel_two_modules() {
        let patch = load_fixture("multi_module_p2b8");
        let f = RenderFeatures::extract(&patch, 120, classic());
        assert_eq!(
            f.panels, 1,
            "two [p2b8] sections share one controller panel"
        );
        assert_eq!(f.modules, 2, "two circuit instances subdivide into modules");
        // 18 components per instance, widest module 18 → 18×16+4.
        assert_eq!(f.min_width, 18 * COMPONENT_WIDTH + 4);
    }

    // ── min_width / overflow ────────────────────────────────────────────────

    #[test]
    fn min_width_is_landscape_natural_width() {
        // P2B8: 10 visible × 16 + 2 = 162; CV I/O: 4 × 16 + 2 = 66 → 228.
        let patch = load_fixture("arpeggio1");
        let f = RenderFeatures::extract(&patch, 120, classic());
        assert_eq!(f.min_width, 162 + 66);
    }

    #[test]
    fn overflow_is_shortfall_against_min_width() {
        let patch = load_fixture("arpeggio1");
        assert_eq!(
            RenderFeatures::extract(&patch, 80, classic()).overflow_cols,
            228 - 80
        );
        assert_eq!(
            RenderFeatures::extract(&patch, 100, classic()).overflow_cols,
            228 - 100
        );
        assert_eq!(
            RenderFeatures::extract(&patch, 120, classic()).overflow_cols,
            228 - 120
        );
    }

    #[test]
    fn native_fit_width_has_zero_overflow() {
        // influence_outlier: the [p2b8] section expands to the P2B8 defaults
        // (B1.1-B1.8, L1.1-L1.8, P1.1, P1.2 — none folded: no `led = L.N`
        // references them), plus E4.4 (Encoder) and M4.2 (Knob) on Controller
        // 4 → 20 visible. Panels: P2B8 (18 comps) → 18×16+2 = 290; Controller
        // 4 (2 comps) → 2×16+2 = 34 → min_width = 324.
        let patch = load_fixture("influence_outlier");
        for width in [324u16, 400, 500] {
            let f = RenderFeatures::extract(&patch, width, classic());
            assert_eq!(f.min_width, 324);
            assert_eq!(f.overflow_cols, 0, "overflow at width {width}");
        }
        let f = RenderFeatures::extract(&patch, 52, classic());
        assert_eq!(f.min_width, 324);
        assert_eq!(f.overflow_cols, 324 - 52, "shortfall below native fit");
    }

    // ── fallback rate ───────────────────────────────────────────────────────

    #[test]
    fn no_fallback_at_matrix_widths() {
        for name in [
            "arpeggio1",
            "influence_outlier",
            "multi_module_p2b8",
            "led_pairs_kinds",
        ] {
            let patch = load_fixture(name);
            for width in [80, 100, 120] {
                let f = RenderFeatures::extract(&patch, width, classic());
                assert_eq!(f.fallback_rate, 0.0, "{name} at {width} cols");
            }
        }
    }

    #[test]
    fn narrow_width_forces_unboxed_fallback() {
        // At width 8 the panels pane is 60% ≈ 5 cols minus borders → grid
        // width < BOX_MIN_WIDTH → every visible cell degrades to unboxed.
        let patch = load_fixture("arpeggio1");
        let f = RenderFeatures::extract(&patch, 8, classic());
        assert!(f.fallback_rate > 0.0, "expected forced fallback at 8 cols");
        // Wide enough for the full cell → boxed again.
        let f = RenderFeatures::extract(&patch, 80, classic());
        assert_eq!(f.fallback_rate, 0.0);
    }

    // ── sidebar / minimap ───────────────────────────────────────────────────

    #[test]
    fn sidebar_and_minimap_visibility_cascade() {
        let patch = load_fixture("arpeggio1");
        // 80: source pane = 32 < 40 → sidebar and minimap hidden.
        let f = RenderFeatures::extract(&patch, 80, classic());
        assert!(f.sidebar_hidden, "sidebar hidden at 80 (source pane 32)");
        assert!(f.minimap_hidden, "minimap hidden at 80");
        // 100: source pane = 40 ≥ 40 → sidebar shown; remaining
        // 40 − 20 (sidebar) − 3 (minimap) = 17 < 20 → minimap still hidden.
        let f = RenderFeatures::extract(&patch, 100, classic());
        assert!(!f.sidebar_hidden, "sidebar visible at 100 (source pane 40)");
        assert!(f.minimap_hidden, "minimap squeezed out at 100 (17 < 20)");
        // 120: source pane = 48; remaining 48 − 20 − 3 = 25 ≥ 20 → both shown.
        let f = RenderFeatures::extract(&patch, 120, classic());
        assert!(!f.sidebar_hidden, "sidebar visible at 120");
        assert!(!f.minimap_hidden, "minimap visible at 120");
    }

    #[test]
    fn sidebar_needs_patch_sections() {
        // A sections-less patch never shows the sidebar, even wide.
        let mut patch = load_fixture("arpeggio1");
        patch.sections.clear();
        let f = RenderFeatures::extract(&patch, 120, classic());
        assert!(f.sidebar_hidden, "no sections → sidebar hidden at 120");
        assert!(!f.minimap_hidden, "minimap still visible (content present)");
    }

    // ── contrast ────────────────────────────────────────────────────────────

    #[test]
    fn min_contrast_known_values() {
        let patch = load_fixture("arpeggio1");
        // Classic: led Red (255,0,0) → 5.252; muted DarkGray → 5.317.
        let f = RenderFeatures::extract(&patch, 120, classic());
        let classic_min = f.min_contrast.expect("classic resolves");
        assert!(
            (classic_min - 5.252).abs() < 0.01,
            "classic min {classic_min}"
        );
        // Mono: shift groups are auto-assigned from the controller number
        // (patch.rs:814, design Decision 2c), so arpeggio1's components pull
        // shift1..shift4 into the co-occurring set — and mono's shift4 is
        // Black, i.e. contrast 1.0 on the assumed black background. This is
        // the exact mono failure the detector exists for.
        let f = RenderFeatures::extract(&patch, 120, mono());
        assert_eq!(
            f.min_contrast,
            Some(1.0),
            "mono shift4 Black fails contrast"
        );
        // Terminal: every token is Reset → unresolvable → None.
        let f = RenderFeatures::extract(&patch, 120, terminal());
        assert_eq!(f.min_contrast, None, "terminal owns its colors");
    }

    #[test]
    fn mono_shift_black_fails_contrast() {
        // shift4 is Black in mono → Black on the assumed black background is
        // contrast 1.0 — the mono-palette failure the detector exists for.
        let patch = shift_patch();
        let f = RenderFeatures::extract(&patch, 120, mono());
        assert_eq!(f.min_contrast, Some(1.0));
    }

    // ── matrix + determinism ────────────────────────────────────────────────

    #[test]
    fn matrix_arpeggio_80_100_120_all_themes() {
        let patch = load_fixture("arpeggio1");
        for theme in [classic(), mono(), terminal()] {
            for width in [80u16, 100, 120] {
                let f = RenderFeatures::extract(&patch, width, theme);
                assert_eq!(f.width, width);
                assert_eq!(f.components, 14);
                // Same geometry for every theme; only contrast differs.
                assert_eq!(f.overflow_cols, 228 - width);
                assert_eq!(f.fallback_rate, 0.0);
            }
        }
    }

    #[test]
    fn extract_is_deterministic_on_repeat() {
        let patch = load_fixture("influence_outlier");
        for theme in [classic(), mono(), terminal()] {
            for width in [80u16, 100, 120] {
                let a = RenderFeatures::extract(&patch, width, theme);
                let b = RenderFeatures::extract(&patch, width, theme);
                assert_eq!(a, b, "repeat extraction at {width} ({theme:?})");
            }
        }
    }

    // ── scorer: scored outliers (task 2.2) ──────────────────────────────────

    #[test]
    fn scored_outlier_overflow_channel() {
        // arpeggio1 at 80 cols: min_width 228 → overflow 148 → overflow band.
        let patch = load_fixture("arpeggio1");
        let f = RenderFeatures::extract(&patch, 80, classic());
        let out = score_render(&f)
            .expect("artifact parses")
            .expect("degraded at 80");
        assert_eq!(out.channel, DegradeChannel::Overflow);
        assert_eq!(out.recommended_width, 228);
    }

    #[test]
    fn scored_outlier_contrast_channel() {
        // mono at native fit (width == min_width): overflow 0, so the width
        // band is silent — the contrast band fires on shift4=Black (1.0).
        let patch = load_fixture("arpeggio1");
        let f = RenderFeatures::extract(&patch, 228, mono());
        let out = score_render(&f)
            .expect("artifact parses")
            .expect("mono contrast fails");
        assert_eq!(out.channel, DegradeChannel::Contrast);
        assert_eq!(out.min_contrast, Some(1.0));
        assert_eq!(out.recommended_width, 228);
        // Sanity: the same patch in classic at native fit is clean.
        let f = RenderFeatures::extract(&patch, 228, classic());
        assert_eq!(score_render(&f).expect("artifact parses"), None);
    }

    #[test]
    fn clean_render_scores_none() {
        // Native fit (width == min_width), classic: no channel fires.
        let patch = load_fixture("influence_outlier");
        let f = RenderFeatures::extract(&patch, 324, classic());
        assert_eq!(score_render(&f).expect("artifact parses"), None);
        // Native fit, terminal: contrast unresolvable, width clean.
        let f = RenderFeatures::extract(&patch, 324, terminal());
        assert_eq!(score_render(&f).expect("artifact parses"), None);
    }

    // ── scorer: fallback + invariant guards (design D5) ─────────────────────

    fn clean_features() -> RenderFeatures {
        RenderFeatures {
            width: 120,
            components: 14,
            panels: 2,
            modules: 2,
            min_width: 100,
            overflow_cols: 0,
            fallback_rate: 0.0,
            sidebar_hidden: false,
            minimap_hidden: false,
            min_contrast: Some(10.0),
        }
    }

    #[test]
    fn fallback_miss_clean_row_scores_none() {
        // No band matches (empty set) and overflow == 0 → clean (baseline
        // is clean, D5 guard 2).
        assert_eq!(score_with_bands(&clean_features(), &[]), None);
    }

    #[test]
    fn fallback_miss_flags_width_degradation() {
        // No band matches but width < min_width → D5 miss → heuristic
        // fallback flags via the Fallback channel (guard 3).
        let mut f = clean_features();
        f.min_width = 120;
        f.overflow_cols = 20;
        let out = score_with_bands(&f, &[]).expect("fallback flags width degradation");
        assert_eq!(out.channel, DegradeChannel::Fallback);
        assert_eq!(out.recommended_width, 120);
    }

    #[test]
    fn native_fit_never_flagged_by_width_channel() {
        // Guard 1: an aggressive overflow band cannot fire at/above native
        // fit because overflow_cols == 0 exactly there (structural).
        let band = RenderBand {
            feature: "overflow_cols".into(),
            min: 1.0,
            max: 1e9,
            degraded: true,
        };
        assert_eq!(score_with_bands(&clean_features(), &[band]), None);
    }

    #[test]
    fn baseline_clean_never_flagged() {
        // Guard 2: with the full embedded table, an overflow-clean row is
        // never flagged — including terminal (contrast NA sentinel never
        // matches the contrast band).
        let patch = load_fixture("influence_outlier");
        let f = RenderFeatures::extract(&patch, 324, terminal());
        assert_eq!(score_render(&f).expect("artifact parses"), None);
    }

    // ── schema drift (design D1: breaks a test, not a user session) ────────

    fn artifact_json(version: u32, features: &[&str], factor: f64) -> String {
        format!(
            r#"{{"version":{version}, "feature_names":{features:?}, "degraded_bands":[], "baseline_min_width_factor":{factor}, "seed":42}}"#
        )
    }

    #[test]
    fn schema_drift_detected_on_bad_json() {
        assert!(parse_artifact("not json").is_err());
    }

    #[test]
    fn schema_drift_detected_on_version_mismatch() {
        assert!(parse_artifact(&artifact_json(2, &FEATURE_NAMES, 1.0)).is_err());
    }

    #[test]
    fn schema_drift_detected_on_feature_names_mismatch() {
        assert!(parse_artifact(&artifact_json(1, &["components"], 1.0)).is_err());
    }

    #[test]
    fn schema_drift_detected_on_baseline_factor_change() {
        assert!(parse_artifact(&artifact_json(1, &FEATURE_NAMES, 0.5)).is_err());
    }

    #[test]
    fn valid_artifact_parses_and_feature_names_match_extractor() {
        let artifact = parse_artifact(RENDER_ARTIFACT_JSON).expect("embedded artifact is valid");
        assert_eq!(artifact.version, 1);
        assert_eq!(artifact.feature_names, FEATURE_NAMES);
        assert!(
            artifact.degraded_bands.iter().all(|b| b.degraded),
            "fitter emits only flag bands"
        );
    }

    // ── Python ↔ Rust extractor agreement on the committed corpus ───────────

    #[test]
    fn python_rust_extractor_agreement_on_corpus() {
        const CSV: &str = include_str!("../corpus/rendermetrics.csv");
        let mut lines = CSV.lines();
        assert_eq!(
            lines.next().unwrap(),
            "patch,width,theme,components,panels,modules,min_width,overflow_cols,\
fallback_rate,sidebar_hidden,minimap_hidden,min_contrast,degraded"
        );
        let mut checked = 0usize;
        for line in lines {
            let cols: Vec<&str> = line.split(',').collect();
            assert_eq!(cols.len(), 13, "malformed corpus row: {line}");
            let patch = load_fixture(cols[0]);
            let width: u16 = cols[1].parse().unwrap();
            let theme = theme::resolve(cols[2]);
            let f = RenderFeatures::extract(&patch, width, theme);

            let approx = |a: f64, b: &str| (a - b.parse::<f64>().unwrap()).abs() < 1e-3;
            assert_eq!(
                f.components,
                cols[3].parse::<usize>().unwrap(),
                "components {line}"
            );
            assert_eq!(f.panels, cols[4].parse::<usize>().unwrap(), "panels {line}");
            assert_eq!(
                f.modules,
                cols[5].parse::<usize>().unwrap(),
                "modules {line}"
            );
            assert_eq!(
                f.min_width,
                cols[6].parse::<u16>().unwrap(),
                "min_width {line}"
            );
            assert_eq!(
                f.overflow_cols,
                cols[7].parse::<u16>().unwrap(),
                "overflow {line}"
            );
            assert!(
                approx(f.fallback_rate as f64, cols[8]),
                "fallback_rate {line}"
            );
            assert_eq!(
                u8::from(f.sidebar_hidden),
                cols[9].parse::<u8>().unwrap(),
                "sidebar_hidden {line}"
            );
            assert_eq!(
                u8::from(f.minimap_hidden),
                cols[10].parse::<u8>().unwrap(),
                "minimap_hidden {line}"
            );
            match f.min_contrast {
                None => assert!(
                    cols[11].is_empty(),
                    "rust None but csv {}: {line}",
                    cols[11]
                ),
                Some(v) => assert!(approx(v as f64, cols[11]), "min_contrast {line}"),
            }
            // Label rule agreement (auditable D3 semantics): the Python label
            // is the union of the three degradation channels.
            let rust_label = (f.overflow_cols > 0
                || f.fallback_rate > 0.0
                || f.min_contrast.is_some_and(|c| c < 4.5)) as u8;
            assert_eq!(
                rust_label,
                cols[12].parse::<u8>().unwrap(),
                "degraded {line}"
            );
            checked += 1;
        }
        assert!(
            checked >= 300,
            "agreement checked only {checked} corpus rows"
        );
    }
}
