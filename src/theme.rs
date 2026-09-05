use std::cell::RefCell;
use std::sync::Mutex;

use ratatui::style::Color;

/// Semantic color tokens for the whole UI; rendering reads these instead of
/// hardcoded `Color::` literals so a theme swap restyles every panel at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub button: Color,
    pub switch: Color,
    pub knob: Color,
    pub cv_in: Color,
    pub cv_out: Color,
    pub led: Color,
    /// The amber LED strip next to each fader on a fader module's faceplate
    /// (p8s8 Faderbank, m4 motorfader). Its own token so the physical-view
    /// fader marker stays distinguishable from knobs/encoders and from the
    /// plain `led` token (design D1).
    pub fader_led_bar: Color,
    pub shift1: Color,
    pub shift2: Color,
    pub shift3: Color,
    pub shift4: Color,
    pub accent: Color,
    pub muted: Color,
    pub text: Color,
    pub viewer_key: Color,
    pub status_bg: Color,
    pub focus_border: Color,
    pub occurrence_highlight: Color,
    pub modifier_boolean: Color,
    pub modifier_exact: Color,
    pub minimap_occurrence: Color,
    pub minimap_modifier_boolean: Color,
    pub minimap_modifier_exact: Color,
    pub minimap_combined: Color,
    pub graph_node_border: Color,
    pub graph_node_title: Color,
    pub graph_port_input: Color,
    pub graph_port_output: Color,
    pub graph_cluster_border: Color,
    pub graph_cluster_title: Color,
    /// Background of the kitty-gfx graph canvas (design D9): the image is
    /// opaque (`f=32`, design D5), so this token paints the whole graph surface
    /// under the nodes and cables. `Black` blends with the terminal-default
    /// dark background the box-drawing path inherits, so the image reads as
    /// the same surface, not a band.
    pub graph_canvas_bg: Color,
    /// Body fill of a graph node in the kitty-gfx image path. The box-drawing
    /// path leaves node interiors transparent, so this token only affects the
    /// image path — a muted panel under the border and title.
    pub graph_node_fill: Color,
    /// Cable edge color by inferred kind (design D8): control, audio, midi,
    /// and the unknown fallback, plus the topology-error highlight.
    pub graph_edge_control: Color,
    pub graph_edge_audio: Color,
    pub graph_edge_midi: Color,
    pub graph_edge_unknown: Color,
    pub graph_edge_error: Color,
    pub graph_node_highlight: Color,
    pub graph_node_dim: Color,
    pub graph_edge_highlight: Color,
    pub graph_edge_dim: Color,
    pub graph_edge_diff_added: Color,
    pub graph_edge_diff_removed: Color,
    /// Cable latency ramp (design D2): 5 cold→hot stops used to color
    /// non-error cables by forward-loop latency when `App.latency_coloring` is
    /// on. `graph_edge_latency_0` is the lowest-latency end, `_4` the hottest
    /// (back-edge) end.
    pub graph_edge_latency_0: Color,
    pub graph_edge_latency_1: Color,
    pub graph_edge_latency_2: Color,
    pub graph_edge_latency_3: Color,
    pub graph_edge_latency_4: Color,
    /// Descriptive text color for the graph latency legend/status line.
    pub graph_edge_latency_legend: Color,
    /// Physical skeleton reference (design D7): module outline, element cell,
    /// and in/out port markers. Dedicated tokens so the skeleton render stops
    /// borrowing graph tokens (task 3.2; 4.1 swaps the renderer onto these).
    pub physical_skeleton_module_outline: Color,
    pub physical_skeleton_cell: Color,
    pub physical_skeleton_port_in: Color,
    pub physical_skeleton_port_out: Color,
    /// DB8E OLED display placeholder (design DB8E): border + centered state
    /// text for the 128×64 upper-band display. Muted neutral so the frame
    /// reads as a display surface, not an accent or error.
    pub display_placeholder: Color,
    pub validation_error: Color,
    pub validation_warning: Color,
    pub validation_hint: Color,
    pub validation_modal_border: Color,
    pub validation_selected_bg: Color,
    /// Advisory render-outlier status hint (design D5): the recommendation
    /// span for a predicted render degradation. Distinct from error surfaces —
    /// the warning is advisory (like topology findings), never gating.
    pub render_outlier_warning: Color,
    /// Optimizer menu (`g o`, design D5): border + selected-row background,
    /// distinct from the validation modal since the menu is advisory (a
    /// preview/export tool), not an error surface.
    pub optimizer_modal_border: Color,
    pub optimizer_selected_bg: Color,
    /// Optimizer weight readout (design D5): the `w = x.x` header span plus the
    /// per-candidate weighted-objective `obj` label. An accent value so the
    /// weight the menu is scored on stays distinct from the muted candidate
    /// values.
    pub optimizer_weight: Color,
    /// Help modal (`?`, design D5): border + key-column background, following
    /// the `optimizer_modal_*` precedent (a neutral informational surface, not
    /// an error surface).
    pub help_modal_border: Color,
    pub help_modal_selected_bg: Color,
}

impl Theme {
    /// The palette shipped before theming existed; must stay byte-identical
    /// to the colors previously hardcoded in `ui.rs`.
    pub const fn classic() -> Self {
        Self {
            button: Color::White,
            // Byte-identical to the previous button-color rendering of Switch
            // cells so the classic palette keeps existing snapshots unchanged.
            switch: Color::White,
            knob: Color::Magenta,
            cv_in: Color::Cyan,
            cv_out: Color::Green,
            led: Color::Red,
            // ANSI-16 yellow reads as the fader strip's amber on most terminals.
            fader_led_bar: Color::Yellow,
            shift1: Color::Yellow,
            shift2: Color::Cyan,
            shift3: Color::Magenta,
            shift4: Color::Green,
            accent: Color::Blue,
            muted: Color::DarkGray,
            text: Color::Reset,
            viewer_key: Color::Cyan,
            status_bg: Color::DarkGray,
            focus_border: Color::Yellow,
            occurrence_highlight: Color::Yellow,
            modifier_boolean: Color::Cyan,
            modifier_exact: Color::Magenta,
            minimap_occurrence: Color::Yellow,
            minimap_modifier_boolean: Color::Cyan,
            minimap_modifier_exact: Color::Magenta,
            minimap_combined: Color::Magenta,
            graph_node_border: Color::White,
            graph_node_title: Color::Yellow,
            graph_port_input: Color::Cyan,
            graph_port_output: Color::Green,
            graph_cluster_border: Color::Blue,
            graph_cluster_title: Color::Blue,
            graph_canvas_bg: Color::Black,
            graph_node_fill: Color::DarkGray,
            graph_edge_control: Color::Cyan,
            graph_edge_audio: Color::Green,
            graph_edge_midi: Color::Magenta,
            graph_edge_unknown: Color::DarkGray,
            graph_edge_error: Color::Red,
            graph_node_highlight: Color::Yellow,
            graph_node_dim: Color::Gray,
            graph_edge_highlight: Color::White,
            graph_edge_dim: Color::DarkGray,
            graph_edge_diff_added: Color::Green,
            graph_edge_diff_removed: Color::Magenta,
            // Cold (blue) → hot (red) latency ramp through the ANSI-16 hues.
            graph_edge_latency_0: Color::Blue,
            graph_edge_latency_1: Color::Cyan,
            graph_edge_latency_2: Color::Green,
            graph_edge_latency_3: Color::Yellow,
            graph_edge_latency_4: Color::Red,
            graph_edge_latency_legend: Color::Blue,
            // Mirrors the graph tokens the 3.1 skeleton renderer reused, so the
            // 4.1 swap onto these stays visually neutral in classic.
            physical_skeleton_module_outline: Color::White,
            physical_skeleton_cell: Color::Cyan,
            physical_skeleton_port_in: Color::Cyan,
            physical_skeleton_port_out: Color::Green,
            display_placeholder: Color::DarkGray,
            validation_error: Color::Red,
            validation_warning: Color::Yellow,
            validation_hint: Color::Cyan,
            validation_modal_border: Color::Red,
            validation_selected_bg: Color::DarkGray,
            // Advisory render-outlier hint: warning yellow, same family as
            // validation_warning but never an error surface.
            render_outlier_warning: Color::Yellow,
            optimizer_modal_border: Color::Blue,
            optimizer_selected_bg: Color::DarkGray,
            // Same accent family as graph_node_title: the weight readout is an
            // accent value, not a muted statistic.
            optimizer_weight: Color::Yellow,
            // Help modal: neutral informational surface — blue border like the
            // optimizer, dark-gray key column so keys read as a distinct band.
            help_modal_border: Color::Blue,
            help_modal_selected_bg: Color::DarkGray,
        }
    }

    /// Every token as `Color::Reset`, letting each user's terminal pick the
    /// actual colors (works with custom schemes and low-color terminals).
    pub const fn terminal() -> Self {
        Self {
            button: Color::Reset,
            switch: Color::Reset,
            knob: Color::Reset,
            cv_in: Color::Reset,
            cv_out: Color::Reset,
            led: Color::Reset,
            fader_led_bar: Color::Reset,
            shift1: Color::Reset,
            shift2: Color::Reset,
            shift3: Color::Reset,
            shift4: Color::Reset,
            accent: Color::Reset,
            muted: Color::Reset,
            text: Color::Reset,
            viewer_key: Color::Reset,
            status_bg: Color::Reset,
            focus_border: Color::Reset,
            occurrence_highlight: Color::Reset,
            modifier_boolean: Color::Reset,
            modifier_exact: Color::Reset,
            minimap_occurrence: Color::Reset,
            minimap_modifier_boolean: Color::Reset,
            minimap_modifier_exact: Color::Reset,
            minimap_combined: Color::Reset,
            graph_node_border: Color::Reset,
            graph_node_title: Color::Reset,
            graph_port_input: Color::Reset,
            graph_port_output: Color::Reset,
            graph_cluster_border: Color::Reset,
            graph_cluster_title: Color::Reset,
            // `Reset` would map to white via the rgb hop, wrong for a dark
            // canvas; Black keeps the opaque image dark on any terminal.
            graph_canvas_bg: Color::Black,
            graph_node_fill: Color::DarkGray,
            graph_edge_control: Color::Reset,
            graph_edge_audio: Color::Reset,
            graph_edge_midi: Color::Reset,
            graph_edge_unknown: Color::Reset,
            graph_edge_error: Color::Reset,
            graph_node_highlight: Color::Reset,
            graph_node_dim: Color::Reset,
            graph_edge_highlight: Color::Reset,
            graph_edge_dim: Color::Reset,
            graph_edge_diff_added: Color::Gray,
            graph_edge_diff_removed: Color::DarkGray,
            graph_edge_latency_0: Color::Reset,
            graph_edge_latency_1: Color::Reset,
            graph_edge_latency_2: Color::Reset,
            graph_edge_latency_3: Color::Reset,
            graph_edge_latency_4: Color::Reset,
            graph_edge_latency_legend: Color::Reset,
            physical_skeleton_module_outline: Color::Reset,
            physical_skeleton_cell: Color::Reset,
            physical_skeleton_port_in: Color::Reset,
            physical_skeleton_port_out: Color::Reset,
            display_placeholder: Color::Reset,
            validation_error: Color::Reset,
            validation_warning: Color::Reset,
            validation_hint: Color::Reset,
            validation_modal_border: Color::Reset,
            validation_selected_bg: Color::Reset,
            render_outlier_warning: Color::Reset,
            optimizer_modal_border: Color::Reset,
            optimizer_selected_bg: Color::Reset,
            optimizer_weight: Color::Reset,
            help_modal_border: Color::Reset,
            help_modal_selected_bg: Color::Reset,
        }
    }

    /// Grayscale palette; shift tokens are pairwise-distinct because shift
    /// groups are distinguished by color alone during normal patching.
    pub const fn mono() -> Self {
        Self {
            button: Color::White,
            // DarkGray so switches stay tellable apart from buttons (White)
            // in the grayscale palette.
            switch: Color::DarkGray,
            knob: Color::White,
            cv_in: Color::Gray,
            cv_out: Color::Gray,
            led: Color::White,
            // Mid-gray so the fader bar stays tellable from the White led and
            // knob tokens in the grayscale palette.
            fader_led_bar: Color::Gray,
            shift1: Color::Gray,
            shift2: Color::White,
            shift3: Color::DarkGray,
            shift4: Color::Black,
            accent: Color::White,
            muted: Color::DarkGray,
            text: Color::White,
            viewer_key: Color::Gray,
            status_bg: Color::Black,
            focus_border: Color::White,
            occurrence_highlight: Color::White,
            // Boolean vs exact modifiers share glyph and underline styling, and
            // minimap occurrence vs combined rows share the same block glyph,
            // so each pair needs distinct grays to stay tellable apart.
            modifier_boolean: Color::Gray,
            modifier_exact: Color::White,
            minimap_occurrence: Color::White,
            minimap_modifier_boolean: Color::Gray,
            minimap_modifier_exact: Color::White,
            minimap_combined: Color::Gray,
            graph_node_border: Color::White,
            graph_node_title: Color::White,
            graph_port_input: Color::Gray,
            graph_port_output: Color::White,
            graph_cluster_border: Color::White,
            graph_cluster_title: Color::Gray,
            // Grayscale contrast ladder for the image path: canvas Black <
            // node fill DarkGray < border White.
            graph_canvas_bg: Color::Black,
            graph_node_fill: Color::DarkGray,
            // The four edge kinds plus error must stay pairwise distinct in mono:
            // type/severity is carried by color alone. Only four gray shades exist,
            // so `unknown` falls back to the terminal default (Reset) as neutral.
            graph_edge_control: Color::White,
            graph_edge_audio: Color::Gray,
            graph_edge_midi: Color::DarkGray,
            graph_edge_unknown: Color::Reset,
            graph_edge_error: Color::Black,
            graph_node_highlight: Color::White,
            graph_node_dim: Color::Black,
            graph_edge_highlight: Color::Gray,
            graph_edge_dim: Color::DarkGray,
            graph_edge_diff_added: Color::White,
            graph_edge_diff_removed: Color::Gray,
            // Grayscale latency ramp: four ANSI grays darkening toward the hot
            // end, with `Reset` (bright terminal default) as the hottest stop so
            // back-edges pop against the dim mid-ramp cables. Only four gray
            // shades exist, so Reset is the neutral fifth, mirroring
            // `graph_edge_unknown`.
            graph_edge_latency_0: Color::White,
            graph_edge_latency_1: Color::Gray,
            graph_edge_latency_2: Color::DarkGray,
            graph_edge_latency_3: Color::Black,
            graph_edge_latency_4: Color::Reset,
            graph_edge_latency_legend: Color::Gray,
            // Skeleton outline/cell/ports co-occur on one screen and carry no
            // other distinguishing cue, so they stay pairwise distinct in mono.
            physical_skeleton_module_outline: Color::White,
            physical_skeleton_cell: Color::DarkGray,
            physical_skeleton_port_in: Color::Gray,
            physical_skeleton_port_out: Color::Black,
            display_placeholder: Color::Gray,
            validation_error: Color::White,
            validation_warning: Color::Gray,
            validation_hint: Color::DarkGray,
            validation_modal_border: Color::White,
            validation_selected_bg: Color::Black,
            // Advisory render-outlier hint: brightest gray so the BOLD span
            // stays tellable in the grayscale palette.
            render_outlier_warning: Color::White,
            optimizer_modal_border: Color::White,
            optimizer_selected_bg: Color::Black,
            // Brightest gray so the weight span stays tellable against the
            // Black selected-row background in the grayscale palette.
            optimizer_weight: Color::White,
            // Brightest gray so the key column stays tellable against the
            // Black selected-row background in the grayscale palette.
            help_modal_border: Color::White,
            help_modal_selected_bg: Color::Black,
        }
    }

    /// The five latency-ramp stops in order (cold → hot). A cable's color is
    /// `ramp[round(L / (N×AVG) × (stops−1))]`, clamped to `stops−1`, so the
    /// hottest stop also covers every latency past the normalization.
    pub const fn graph_edge_latency_ramp(&self) -> [Color; 5] {
        [
            self.graph_edge_latency_0,
            self.graph_edge_latency_1,
            self.graph_edge_latency_2,
            self.graph_edge_latency_3,
            self.graph_edge_latency_4,
        ]
    }

    /// The `Color → RGB` hop (design D9): the single source of pixel colors
    /// for the kitty rasterizer. ANSI-16 tokens map to fixed triples (pure
    /// primaries, half-intensity pastels for the `Light*` variants), `Rgb`
    /// passes through, `Indexed` resolves through the standard xterm-256
    /// table, and `Reset` (the "defer to terminal" token) resolves to an
    /// opaque bright white — the kitty canvas is opaque (`f=32` premultiplied
    /// == straight), so a Reset token must still yield an opaque, readable fg
    /// (the `terminal` palette is all Reset, and mono's neutral/hot tokens are
    /// Reset by design). Pure and deterministic: the same token always maps to
    /// the same triple, and every `Color` variant is covered so the pixel path
    /// never panics.
    pub fn rgb(&self, color: Color) -> (u8, u8, u8) {
        match color {
            Color::Reset => (0xff, 0xff, 0xff),
            Color::Black => ansi16_rgb(0),
            Color::Red => ansi16_rgb(1),
            Color::Green => ansi16_rgb(2),
            Color::Yellow => ansi16_rgb(3),
            Color::Blue => ansi16_rgb(4),
            Color::Magenta => ansi16_rgb(5),
            Color::Cyan => ansi16_rgb(6),
            Color::Gray => ansi16_rgb(7),
            Color::DarkGray => ansi16_rgb(8),
            Color::LightRed => ansi16_rgb(9),
            Color::LightGreen => ansi16_rgb(10),
            Color::LightYellow => ansi16_rgb(11),
            Color::LightBlue => ansi16_rgb(12),
            Color::LightMagenta => ansi16_rgb(13),
            Color::LightCyan => ansi16_rgb(14),
            Color::White => ansi16_rgb(15),
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(v) => xterm256_rgb(v),
        }
    }
}

/// The single ANSI-16 → RGB table, shared by the named variants in
/// [`Theme::rgb`] and the 0..=15 head of the xterm-256 map so `Color::Red`
/// and `Color::Indexed(1)` agree. Out-of-range indices degrade to the bright
/// neutral (same as `Reset`) so the hop stays total without panicking.
fn ansi16_rgb(v: u8) -> (u8, u8, u8) {
    match v {
        0 => (0x00, 0x00, 0x00),
        1 => (0xff, 0x00, 0x00),
        2 => (0x00, 0xff, 0x00),
        3 => (0xff, 0xff, 0x00),
        4 => (0x00, 0x00, 0xff),
        5 => (0xff, 0x00, 0xff),
        6 => (0x00, 0xff, 0xff),
        7 => (0x80, 0x80, 0x80),
        8 => (0x40, 0x40, 0x40),
        9 => (0xff, 0x80, 0x80),
        10 => (0x80, 0xff, 0x80),
        11 => (0xff, 0xff, 0x80),
        12 => (0x80, 0x80, 0xff),
        13 => (0xff, 0x80, 0xff),
        14 => (0x80, 0xff, 0xff),
        _ => (0xff, 0xff, 0xff),
    }
}

/// Standard xterm-256 → RGB (deterministic): 16-color head mirroring
/// [`ansi16_rgb`], a 6×6×6 cube for 16..=231, and the 24-step gray ramp for
/// 232..=255. `Theme::rgb` never emits a 256-color token today, but the hop
/// must be total so the pixel path cannot panic on any `Color`.
fn xterm256_rgb(v: u8) -> (u8, u8, u8) {
    match v {
        0..=15 => ansi16_rgb(v),
        16..=231 => {
            let n = v - 16;
            let (r, g, b) = (n / 36, (n % 36) / 6, n % 6);
            (cube_step(r), cube_step(g), cube_step(b))
        }
        _ => {
            let g = 8 + 10 * (v - 232);
            (g, g, g)
        }
    }
}

/// One 6×6×6 cube step: `0, 95, 135, 175, 215, 255`.
fn cube_step(c: u8) -> u8 {
    [0, 95, 135, 175, 215, 255][c as usize]
}

/// User-facing theme names, in presentation order.
pub const THEMES: &[&str] = &["classic", "terminal", "mono"];

const CLASSIC: Theme = Theme::classic();
const TERMINAL: Theme = Theme::terminal();
const MONO: Theme = Theme::mono();

fn is_name_separator(c: char) -> bool {
    c == '-' || c == '_' || c == ' '
}

/// Maps user input to an entry of [`THEMES`] (`Some`) or rejects it (`None`).
/// Case-insensitive; `-`, `_`, and space are interchangeable separators.
pub fn canonical_theme_name(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    THEMES.iter().copied().find(|candidate| {
        let cand_lower = candidate.to_ascii_lowercase();
        cand_lower.len() == lower.len()
            && cand_lower
                .chars()
                .zip(lower.chars())
                .all(|(c, n)| c == n || (is_name_separator(c) && is_name_separator(n)))
    })
}

/// Resolves a user-supplied theme name, falling back to `classic` so a bad
/// config value degrades to the familiar palette instead of failing startup.
pub fn resolve(name: &str) -> &'static Theme {
    match canonical_theme_name(name) {
        Some("terminal") => &TERMINAL,
        Some("mono") => &MONO,
        _ => &CLASSIC,
    }
}

static ACTIVE: Mutex<Option<&'static Theme>> = Mutex::new(None);

thread_local! {
    // Per-thread palette override so parallel tests and the gallery
    // binary can render under different themes without observing each
    // other's global state.
    static TEST_OVERRIDE: RefCell<Option<&'static Theme>> = const { RefCell::new(None) };
}

/// The theme rendering must use. Defaults to `classic` until `init` runs.
pub fn active() -> &'static Theme {
    if let Some(theme) = TEST_OVERRIDE.with(|slot| *slot.borrow()) {
        return theme;
    }
    ACTIVE
        .lock()
        .map(|guard| *guard)
        .ok()
        .flatten()
        .unwrap_or(&CLASSIC)
}

/// Installs the startup-selected theme. A second call is ignored because
/// rendering holds references into the first value.
pub fn init(theme: Theme) {
    // Leak so `active()` can hand out `&'static Theme` without holding the
    // lock across rendering.
    let installed: &'static Theme = Box::leak(Box::new(theme));
    if let Ok(mut guard) = ACTIVE.lock() {
        if guard.is_none() {
            *guard = Some(installed);
        }
    }
}

/// Pins the palette for the calling thread (`None` restores the
/// global/default resolution). Used by theme-sensitive tests and the
/// gallery generator to keep renders independent.
pub fn set_test_theme(theme: Option<Theme>) {
    let leaked = theme.map(|t| &*Box::leak(Box::new(t)));
    TEST_OVERRIDE.with(|slot| *slot.borrow_mut() = leaked);
}

/// Stable per-modifier hue: `hash(token) % 16` over ANSI-16 palette.
/// Pure helper, no Theme mutation, no config key. Deterministic per run.
/// Collisions tolerated. Keeps advisory hue distinct from `graph_edge_error` red
/// by mapping through the 16 ANSI colors.
pub fn modifier_hue(token: &str) -> Color {
    let mut hash: u32 = 0;
    for b in token.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as u32);
    }
    let idx = (hash % 16) as u8;
    match idx {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_matches_previous_hardcoded_colors() {
        let t = Theme::classic();
        assert_eq!(t.button, Color::White);
        assert_eq!(t.switch, Color::White);
        assert_eq!(t.knob, Color::Magenta);
        assert_eq!(t.cv_in, Color::Cyan);
        assert_eq!(t.cv_out, Color::Green);
        assert_eq!(t.led, Color::Red);
        assert_eq!(t.shift1, Color::Yellow);
        assert_eq!(t.shift2, Color::Cyan);
        assert_eq!(t.shift3, Color::Magenta);
        assert_eq!(t.shift4, Color::Green);
        assert_eq!(t.accent, Color::Blue);
        assert_eq!(t.muted, Color::DarkGray);
        assert_eq!(t.text, Color::Reset);
        assert_eq!(t.viewer_key, Color::Cyan);
        assert_eq!(t.status_bg, Color::DarkGray);
        assert_eq!(t.focus_border, Color::Yellow);
        assert_eq!(t.occurrence_highlight, Color::Yellow);
        assert_eq!(t.modifier_boolean, Color::Cyan);
        assert_eq!(t.modifier_exact, Color::Magenta);
        assert_eq!(t.minimap_occurrence, Color::Yellow);
        assert_eq!(t.minimap_modifier_boolean, Color::Cyan);
        assert_eq!(t.minimap_modifier_exact, Color::Magenta);
        assert_eq!(t.minimap_combined, Color::Magenta);
        assert_eq!(t.graph_node_border, Color::White);
        assert_eq!(t.graph_node_title, Color::Yellow);
        assert_eq!(t.graph_port_input, Color::Cyan);
        assert_eq!(t.graph_port_output, Color::Green);
        assert_eq!(t.graph_cluster_border, Color::Blue);
        assert_eq!(t.graph_cluster_title, Color::Blue);
        assert_eq!(t.graph_edge_control, Color::Cyan);
        assert_eq!(t.graph_edge_audio, Color::Green);
        assert_eq!(t.graph_edge_midi, Color::Magenta);
        assert_eq!(t.graph_edge_unknown, Color::DarkGray);
        assert_eq!(t.graph_edge_error, Color::Red);
    }

    #[test]
    fn terminal_is_all_reset() {
        let t = Theme::terminal();
        for color in [
            t.button,
            t.switch,
            t.knob,
            t.cv_in,
            t.cv_out,
            t.led,
            t.fader_led_bar,
            t.shift1,
            t.shift2,
            t.shift3,
            t.shift4,
            t.accent,
            t.muted,
            t.text,
            t.viewer_key,
            t.status_bg,
            t.focus_border,
            t.occurrence_highlight,
            t.modifier_boolean,
            t.modifier_exact,
            t.minimap_occurrence,
            t.minimap_modifier_boolean,
            t.minimap_modifier_exact,
            t.minimap_combined,
            t.graph_node_border,
            t.graph_node_title,
            t.graph_port_input,
            t.graph_port_output,
            t.graph_cluster_border,
            t.graph_cluster_title,
            t.graph_edge_control,
            t.graph_edge_audio,
            t.graph_edge_midi,
            t.graph_edge_unknown,
            t.graph_edge_error,
            // `graph_edge_diff_added/removed` are intentionally Gray/DarkGray
            // (diff needs distinguishability even in the colorless terminal
            // theme); latency ramp + legend are all Reset.
            t.graph_edge_latency_0,
            t.graph_edge_latency_1,
            t.graph_edge_latency_2,
            t.graph_edge_latency_3,
            t.graph_edge_latency_4,
            t.graph_edge_latency_legend,
            t.physical_skeleton_module_outline,
            t.physical_skeleton_cell,
            t.physical_skeleton_port_in,
            t.physical_skeleton_port_out,
            t.display_placeholder,
        ] {
            assert_eq!(color, Color::Reset);
        }
        // The two diff tokens are the documented terminal exceptions: distinct
        // grays so added/removed cables stay tellable at the terminal's
        // default palette.
        assert_eq!(t.graph_edge_diff_added, Color::Gray);
        assert_eq!(t.graph_edge_diff_removed, Color::DarkGray);
    }

    #[test]
    fn every_palette_exposes_five_ramp_stops_and_a_legend_token() {
        for theme in [Theme::classic(), Theme::terminal(), Theme::mono()] {
            assert_eq!(theme.graph_edge_latency_ramp().len(), 5);
        }
        // Legend token present per palette.
        assert_eq!(Theme::classic().graph_edge_latency_legend, Color::Blue);
        assert_eq!(Theme::terminal().graph_edge_latency_legend, Color::Reset);
        assert_eq!(Theme::mono().graph_edge_latency_legend, Color::Gray);
    }

    #[test]
    fn classic_latency_ramp_orders_blue_to_red_by_hue() {
        let ramp = Theme::classic().graph_edge_latency_ramp();
        assert_eq!(ramp[0], Color::Blue, "cold end must be blue");
        assert_eq!(ramp[4], Color::Red, "hot end must be red");
        // Monotonic cold→hot progression: each stop steps toward red through
        // cyan → green → yellow.
        assert_eq!(ramp[1], Color::Cyan);
        assert_eq!(ramp[2], Color::Green);
        assert_eq!(ramp[3], Color::Yellow);
    }

    #[test]
    fn mono_latency_ramp_stops_are_pairwise_distinct_and_colorless() {
        let ramp = Theme::mono().graph_edge_latency_ramp();
        for (i, a) in ramp.iter().enumerate() {
            for b in &ramp[i + 1..] {
                assert_ne!(a, b, "latency ramp stops must be pairwise distinct");
            }
        }
        // All stops are grayscale (or the neutral Reset fallback), never a hue.
        for stop in ramp {
            assert!(
                matches!(
                    stop,
                    Color::Black | Color::DarkGray | Color::Gray | Color::White | Color::Reset
                ),
                "mono latency stop {stop:?} must be grayscale/neutral"
            );
        }
    }

    #[test]
    fn switch_token_resolves_per_palette() {
        let classic = Theme::classic();
        let terminal = Theme::terminal();
        let mono = Theme::mono();
        // Classic keeps the previous switch/button color byte-identical so
        // existing snapshots don't change.
        assert_eq!(classic.switch, Color::White);
        assert_eq!(classic.switch, classic.button);
        // Terminal defers every token to the user's terminal.
        assert_eq!(terminal.switch, Color::Reset);
        // Mono gives switches their own shade, distinct from button's gray.
        assert_eq!(mono.switch, Color::DarkGray);
        assert_ne!(mono.switch, mono.button);
    }

    #[test]
    fn fader_led_bar_resolves_per_palette() {
        // Physical-view fader LED bar (design D1): classic amber (ANSI
        // yellow), terminal defers to the terminal, mono a mid-gray so the bar
        // stays tellable from the White led/knob tokens.
        assert_eq!(Theme::classic().fader_led_bar, Color::Yellow);
        assert_eq!(Theme::terminal().fader_led_bar, Color::Reset);
        assert_eq!(Theme::mono().fader_led_bar, Color::Gray);
        assert_ne!(Theme::mono().fader_led_bar, Theme::mono().led);
        assert_ne!(Theme::mono().fader_led_bar, Theme::mono().knob);
    }

    #[test]
    fn mono_shift_tokens_are_pairwise_distinct() {
        let t = Theme::mono();
        let shifts = [t.shift1, t.shift2, t.shift3, t.shift4];
        for (i, a) in shifts.iter().enumerate() {
            for b in &shifts[i + 1..] {
                assert_ne!(a, b, "shift tokens must be pairwise distinct");
            }
        }
    }

    #[test]
    fn mono_distinct_where_signals_share_glyph_and_modifier() {
        // These pairs render with identical glyphs/modifiers in the minimap
        // and source view, so color is the only remaining distinguishing cue.
        let t = Theme::mono();
        assert_ne!(t.modifier_boolean, t.modifier_exact);
        assert_ne!(t.minimap_occurrence, t.minimap_combined);
        assert_ne!(t.minimap_modifier_boolean, t.minimap_modifier_exact);
    }

    #[test]
    fn mono_edge_tokens_are_pairwise_distinct() {
        let t = Theme::mono();
        let edges = [
            t.graph_edge_control,
            t.graph_edge_audio,
            t.graph_edge_midi,
            t.graph_edge_unknown,
            t.graph_edge_error,
        ];
        for (i, a) in edges.iter().enumerate() {
            for b in &edges[i + 1..] {
                assert_ne!(a, b, "edge kind/error tokens must be pairwise distinct");
            }
        }
    }

    #[test]
    fn themes_catalog_lists_builtin_names_in_order() {
        assert_eq!(THEMES, &["classic", "terminal", "mono"]);
    }

    #[test]
    fn canonical_name_accepts_any_case_and_separators() {
        assert_eq!(canonical_theme_name("classic"), Some("classic"));
        assert_eq!(canonical_theme_name("Classic"), Some("classic"));
        assert_eq!(canonical_theme_name("TERMINAL"), Some("terminal"));
        assert_eq!(canonical_theme_name("MONO"), Some("mono"));
    }

    #[test]
    fn canonical_name_rejects_unknown_and_empty_input() {
        assert_eq!(canonical_theme_name("neon"), None);
        assert_eq!(canonical_theme_name(""), None);
    }

    #[test]
    fn resolve_maps_known_names_and_falls_back_to_classic() {
        assert_eq!(resolve("terminal"), &Theme::terminal());
        assert_eq!(resolve("Mono"), &Theme::mono());
        assert_eq!(resolve("no-such-theme"), &Theme::classic());
        assert_eq!(resolve(""), &Theme::classic());
    }

    #[test]
    fn active_defaults_to_classic_and_init_overrides_it() {
        set_test_theme(None);
        assert_eq!(*active(), Theme::classic());
        set_test_theme(Some(Theme::terminal()));
        assert_eq!(*active(), Theme::terminal());
        set_test_theme(None);
    }

    #[test]
    fn all_palettes_define_render_outlier_warning() {
        // Task 3.1: the advisory render-outlier status-hint token exists in
        // every palette (classic yellow / terminal reset / mono white).
        assert_eq!(Theme::classic().render_outlier_warning, Color::Yellow);
        assert_eq!(Theme::terminal().render_outlier_warning, Color::Reset);
        assert_eq!(Theme::mono().render_outlier_warning, Color::White);
    }

    #[test]
    fn all_palettes_define_skeleton_tokens() {
        // Task 3.2: physical skeleton tokens (design D7) exist in every
        // palette — classic mirrors the graph tokens 3.1 reused so the 4.1
        // swap is neutral, terminal defers to the terminal, mono keeps the
        // co-occurring outline/cell/ports pairwise distinct.
        let classic = Theme::classic();
        let terminal = Theme::terminal();
        let mono = Theme::mono();
        assert_eq!(classic.physical_skeleton_module_outline, Color::White);
        assert_eq!(classic.physical_skeleton_cell, Color::Cyan);
        assert_eq!(classic.physical_skeleton_port_in, Color::Cyan);
        assert_eq!(classic.physical_skeleton_port_out, Color::Green);
        for color in [
            terminal.physical_skeleton_module_outline,
            terminal.physical_skeleton_cell,
            terminal.physical_skeleton_port_in,
            terminal.physical_skeleton_port_out,
        ] {
            assert_eq!(color, Color::Reset);
        }
        let skeleton = [
            mono.physical_skeleton_module_outline,
            mono.physical_skeleton_cell,
            mono.physical_skeleton_port_in,
            mono.physical_skeleton_port_out,
        ];
        for (i, a) in skeleton.iter().enumerate() {
            for b in &skeleton[i + 1..] {
                assert_ne!(a, b, "skeleton tokens must be pairwise distinct in mono");
            }
        }
    }

    #[test]
    fn all_palettes_define_display_placeholder() {
        // Task 1.1 (db8e-oled-display-placeholder): display_placeholder exists in
        // every palette — classic muted neutral, terminal Reset, mono mid-gray
        // distinct from muted/text so the OLED frame stays tellable in grayscale.
        assert_eq!(Theme::classic().display_placeholder, Color::DarkGray);
        assert_eq!(Theme::terminal().display_placeholder, Color::Reset);
        assert_eq!(Theme::mono().display_placeholder, Color::Gray);
        assert_ne!(
            Theme::mono().display_placeholder,
            Theme::mono().muted,
            "mono placeholder must be distinct from muted"
        );
        assert_ne!(
            Theme::mono().display_placeholder,
            Theme::mono().text,
            "mono placeholder must be distinct from text"
        );
        assert_ne!(
            Theme::mono().display_placeholder,
            Theme::mono().physical_skeleton_cell,
            "mono placeholder must be distinct from skeleton cell"
        );
    }

    #[test]
    fn every_token_in_every_palette_maps_to_a_deterministic_rgb_triple() {
        // Task 2.2 + 3.2 verification: every semantic token the UI/graph
        // surface and the rasterizer consume (component kinds, shift groups,
        // graph chrome + canvas + edges, skeleton, validation, optimizer) must
        // resolve through the `Color → RGB` hop without panicking and
        // deterministically (same token → same triple every call), in every
        // palette.
        for theme in [Theme::classic(), Theme::terminal(), Theme::mono()] {
            let tokens = [
                theme.button,
                theme.switch,
                theme.knob,
                theme.cv_in,
                theme.cv_out,
                theme.led,
                theme.fader_led_bar,
                theme.shift1,
                theme.shift2,
                theme.shift3,
                theme.shift4,
                theme.accent,
                theme.muted,
                theme.text,
                theme.viewer_key,
                theme.status_bg,
                theme.focus_border,
                theme.occurrence_highlight,
                theme.modifier_boolean,
                theme.modifier_exact,
                theme.minimap_occurrence,
                theme.minimap_modifier_boolean,
                theme.minimap_modifier_exact,
                theme.minimap_combined,
                theme.graph_node_border,
                theme.graph_node_title,
                theme.graph_port_input,
                theme.graph_port_output,
                theme.graph_cluster_border,
                theme.graph_cluster_title,
                theme.graph_canvas_bg,
                theme.graph_node_fill,
                theme.graph_edge_control,
                theme.graph_edge_audio,
                theme.graph_edge_midi,
                theme.graph_edge_unknown,
                theme.graph_edge_error,
                theme.graph_node_highlight,
                theme.graph_node_dim,
                theme.graph_edge_highlight,
                theme.graph_edge_dim,
                theme.graph_edge_diff_added,
                theme.graph_edge_diff_removed,
                theme.graph_edge_latency_0,
                theme.graph_edge_latency_1,
                theme.graph_edge_latency_2,
                theme.graph_edge_latency_3,
                theme.graph_edge_latency_4,
                theme.graph_edge_latency_legend,
                theme.physical_skeleton_module_outline,
                theme.physical_skeleton_cell,
                theme.physical_skeleton_port_in,
                theme.physical_skeleton_port_out,
                theme.display_placeholder,
                theme.validation_error,
                theme.validation_warning,
                theme.validation_hint,
                theme.validation_modal_border,
                theme.validation_selected_bg,
                theme.render_outlier_warning,
                theme.optimizer_modal_border,
                theme.optimizer_selected_bg,
                theme.optimizer_weight,
                theme.help_modal_border,
                theme.help_modal_selected_bg,
            ];
            for token in tokens {
                let rgb = theme.rgb(token);
                assert_eq!(
                    theme.rgb(token),
                    rgb,
                    "hop must be deterministic for {token:?}"
                );
            }
        }
    }

    #[test]
    fn classic_graph_tokens_resolve_to_expected_rgb() {
        // Task 2.2 verification: a token → expected-RGB table for the classic
        // palette, anchoring the hop's mapping (error red, cable kinds,
        // latency ramp cold→hot, node/cluster chrome).
        let t = Theme::classic();
        assert_eq!(t.rgb(t.graph_edge_error), (0xff, 0x00, 0x00), "error red");
        assert_eq!(t.rgb(t.graph_edge_audio), (0x00, 0xff, 0x00), "audio green");
        assert_eq!(
            t.rgb(t.graph_edge_control),
            (0x00, 0xff, 0xff),
            "control cyan"
        );
        assert_eq!(t.rgb(t.graph_edge_midi), (0xff, 0x00, 0xff), "midi magenta");
        assert_eq!(
            t.rgb(t.graph_edge_unknown),
            (0x40, 0x40, 0x40),
            "unknown dark-gray"
        );
        assert_eq!(t.rgb(t.graph_edge_diff_added), (0x00, 0xff, 0x00));
        assert_eq!(t.rgb(t.graph_edge_diff_removed), (0xff, 0x00, 0xff));
        assert_eq!(t.rgb(t.graph_node_border), (0xff, 0xff, 0xff));
        assert_eq!(t.rgb(t.graph_node_title), (0xff, 0xff, 0x00));
        assert_eq!(t.rgb(t.graph_port_input), (0x00, 0xff, 0xff));
        assert_eq!(t.rgb(t.graph_port_output), (0x00, 0xff, 0x00));
        assert_eq!(t.rgb(t.graph_cluster_border), (0x00, 0x00, 0xff));
        assert_eq!(t.rgb(t.graph_cluster_title), (0x00, 0x00, 0xff));
        assert_eq!(t.rgb(t.graph_node_dim), (0x80, 0x80, 0x80));
        assert_eq!(t.rgb(t.graph_edge_dim), (0x40, 0x40, 0x40));
        assert_eq!(t.rgb(t.graph_edge_highlight), (0xff, 0xff, 0xff));
        assert_eq!(t.rgb(t.graph_node_highlight), (0xff, 0xff, 0x00));
        // Latency ramp cold → hot: blue → cyan → green → yellow → red.
        assert_eq!(t.rgb(t.graph_edge_latency_0), (0x00, 0x00, 0xff));
        assert_eq!(t.rgb(t.graph_edge_latency_1), (0x00, 0xff, 0xff));
        assert_eq!(t.rgb(t.graph_edge_latency_2), (0x00, 0xff, 0x00));
        assert_eq!(t.rgb(t.graph_edge_latency_3), (0xff, 0xff, 0x00));
        assert_eq!(t.rgb(t.graph_edge_latency_4), (0xff, 0x00, 0x00));
        assert_eq!(t.rgb(t.graph_edge_latency_legend), (0x00, 0x00, 0xff));
    }

    #[test]
    fn every_color_variant_maps_without_panic() {
        // Task 2.2 verification: every `Color` variant ratatui 0.29 can emit
        // for these tokens is handled — the 17 ANSI-16 named variants, the
        // `Rgb` passthrough, `Indexed` (0.29's 256-color variant), and `Reset`.
        let t = Theme::classic();
        for variant in [
            Color::Reset,
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
            Color::DarkGray,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::White,
            Color::Rgb(12, 34, 56),
            Color::Indexed(0),
            Color::Indexed(1),
            Color::Indexed(7),
            Color::Indexed(15),
            Color::Indexed(16),
            Color::Indexed(21),
            Color::Indexed(196),
            Color::Indexed(231),
            Color::Indexed(232),
            Color::Indexed(255),
        ] {
            t.rgb(variant); // exhaustive match: no `Color` variant can panic
        }
        // Rgb passes through unchanged; Reset resolves to the opaque neutral.
        assert_eq!(t.rgb(Color::Rgb(12, 34, 56)), (12, 34, 56));
        assert_eq!(t.rgb(Color::Reset), (0xff, 0xff, 0xff));
        // Indexed head mirrors the ANSI-16 table; cube corners and the gray
        // ramp resolve deterministically.
        assert_eq!(t.rgb(Color::Indexed(1)), t.rgb(Color::Red));
        assert_eq!(t.rgb(Color::Indexed(7)), t.rgb(Color::Gray));
        assert_eq!(t.rgb(Color::Indexed(15)), t.rgb(Color::White));
        assert_eq!(t.rgb(Color::Indexed(0)), (0x00, 0x00, 0x00));
        assert_eq!(
            t.rgb(Color::Indexed(21)),
            (0x00, 0x00, 0xff),
            "cube blue corner"
        );
        assert_eq!(
            t.rgb(Color::Indexed(196)),
            (0xff, 0x00, 0x00),
            "cube red corner"
        );
        assert_eq!(
            t.rgb(Color::Indexed(231)),
            (0xff, 0xff, 0xff),
            "cube white corner"
        );
        assert_eq!(
            t.rgb(Color::Indexed(232)),
            (0x08, 0x08, 0x08),
            "gray ramp bottom"
        );
        assert_eq!(
            t.rgb(Color::Indexed(255)),
            (0xee, 0xee, 0xee),
            "gray ramp top"
        );
    }

    #[test]
    fn ansi16_primaries_map_to_expected_rgb() {
        // Task 3.2: the ANSI-16 named variants resolve to their standard RGB
        // triples — pure primaries, half-intensity pastels for the Light*
        // variants, and Reset to the opaque bright neutral the kitty canvas
        // needs (there is no terminal-default fg in an opaque image).
        let t = Theme::classic();
        assert_eq!(t.rgb(Color::Black), (0x00, 0x00, 0x00));
        assert_eq!(t.rgb(Color::Red), (0xff, 0x00, 0x00));
        assert_eq!(t.rgb(Color::Green), (0x00, 0xff, 0x00));
        assert_eq!(t.rgb(Color::Yellow), (0xff, 0xff, 0x00));
        assert_eq!(t.rgb(Color::Blue), (0x00, 0x00, 0xff));
        assert_eq!(t.rgb(Color::Magenta), (0xff, 0x00, 0xff));
        assert_eq!(t.rgb(Color::Cyan), (0x00, 0xff, 0xff));
        assert_eq!(t.rgb(Color::Gray), (0x80, 0x80, 0x80));
        assert_eq!(t.rgb(Color::DarkGray), (0x40, 0x40, 0x40));
        assert_eq!(t.rgb(Color::LightRed), (0xff, 0x80, 0x80));
        assert_eq!(t.rgb(Color::LightGreen), (0x80, 0xff, 0x80));
        assert_eq!(t.rgb(Color::LightYellow), (0xff, 0xff, 0x80));
        assert_eq!(t.rgb(Color::LightBlue), (0x80, 0x80, 0xff));
        assert_eq!(t.rgb(Color::LightMagenta), (0xff, 0x80, 0xff));
        assert_eq!(t.rgb(Color::LightCyan), (0x80, 0xff, 0xff));
        assert_eq!(t.rgb(Color::White), (0xff, 0xff, 0xff));
        assert_eq!(t.rgb(Color::Reset), (0xff, 0xff, 0xff));
    }

    #[test]
    fn indexed_head_agrees_with_named_ansi16_variants() {
        // Task 3.2: xterm indices 0..=15 resolve exactly like the named
        // ANSI-16 variants (index 0 == Black, …, index 15 == White), because
        // the xterm-256 map shares the 0..=15 head with the named variants.
        let t = Theme::classic();
        let named = [
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
            Color::DarkGray,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::White,
        ];
        for (v, color) in named.iter().enumerate() {
            assert_eq!(
                t.rgb(Color::Indexed(v as u8)),
                t.rgb(*color),
                "Indexed({v}) must agree with the named ANSI-16 variant"
            );
        }
    }

    #[test]
    fn indexed_gray_ramp_is_monotonic() {
        // Task 3.2: the 232..=255 gray ramp steps 8 → 238 strictly upwards, so
        // brighter indices always resolve to brighter pixels.
        let t = Theme::classic();
        assert_eq!(
            t.rgb(Color::Indexed(232)),
            (0x08, 0x08, 0x08),
            "ramp bottom"
        );
        let mut prev = t.rgb(Color::Indexed(232)).0;
        for v in 233..=255u8 {
            let g = t.rgb(Color::Indexed(v)).0;
            assert!(g > prev, "gray ramp must increase strictly at {v}");
            prev = g;
        }
        assert_eq!(t.rgb(Color::Indexed(255)), (0xee, 0xee, 0xee), "ramp top");
    }

    #[test]
    fn indexed_full_range_is_total_and_deterministic() {
        // Task 3.2: every xterm-256 index resolves to a triple (never panics)
        // and the same index always resolves to the same triple. The cube
        // (16..=231) and gray (232..=255) regions anchor the structure.
        let t = Theme::classic();
        for v in 0..=255u8 {
            let rgb = t.rgb(Color::Indexed(v));
            assert_eq!(
                t.rgb(Color::Indexed(v)),
                rgb,
                "Indexed({v}) must be deterministic"
            );
        }
        assert_eq!(
            t.rgb(Color::Indexed(16)),
            (0x00, 0x00, 0x00),
            "cube corner black"
        );
        assert_eq!(
            t.rgb(Color::Indexed(17)),
            (0x00, 0x00, 0x5f),
            "cube step 95"
        );
        assert_eq!(
            t.rgb(Color::Indexed(231)),
            (0xff, 0xff, 0xff),
            "cube corner white"
        );
    }

    #[test]
    fn terminal_and_mono_reset_tokens_round_trip_to_opaque_neutral() {
        // The `terminal` palette defers every token to the terminal (`Reset`);
        // the opaque kitty canvas has no "default fg", so Reset must still
        // resolve to an opaque, readable fg.
        let terminal = Theme::terminal();
        for token in [
            terminal.graph_edge_control,
            terminal.graph_edge_audio,
            terminal.graph_edge_error,
            terminal.graph_node_border,
            terminal.graph_node_title,
            terminal.graph_edge_latency_4,
        ] {
            assert_eq!(terminal.rgb(token), (0xff, 0xff, 0xff));
        }
        // mono's neutral `unknown` and hottest latency stop are Reset too —
        // they resolve bright so they stay visible against the dark canvas.
        let mono = Theme::mono();
        assert_eq!(mono.rgb(mono.graph_edge_unknown), (0xff, 0xff, 0xff));
        assert_eq!(mono.rgb(mono.graph_edge_latency_4), (0xff, 0xff, 0xff));
        // The four ANSI grays mono uses for edge kinds stay distinct in pixel
        // space (Reset doubles as the fifth shade, colliding with White — the
        // canvas has no terminal-default fg to defer to).
        let grays = [
            mono.rgb(mono.graph_edge_control), // White
            mono.rgb(mono.graph_edge_audio),   // Gray
            mono.rgb(mono.graph_edge_midi),    // DarkGray
            mono.rgb(mono.graph_edge_error),   // Black
        ];
        for (i, a) in grays.iter().enumerate() {
            for b in &grays[i + 1..] {
                assert_ne!(a, b, "mono edge tokens must stay distinct in RGB");
            }
        }
    }
}
