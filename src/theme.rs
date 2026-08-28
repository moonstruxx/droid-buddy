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
    pub validation_error: Color,
    pub validation_warning: Color,
    pub validation_hint: Color,
    pub validation_modal_border: Color,
    pub validation_selected_bg: Color,
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
            validation_error: Color::Red,
            validation_warning: Color::Yellow,
            validation_hint: Color::Cyan,
            validation_modal_border: Color::Red,
            validation_selected_bg: Color::DarkGray,
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
            validation_error: Color::Reset,
            validation_warning: Color::Reset,
            validation_hint: Color::Reset,
            validation_modal_border: Color::Reset,
            validation_selected_bg: Color::Reset,
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
            validation_error: Color::White,
            validation_warning: Color::Gray,
            validation_hint: Color::DarkGray,
            validation_modal_border: Color::White,
            validation_selected_bg: Color::Black,
        }
    }
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
        ] {
            assert_eq!(color, Color::Reset);
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
}
