use std::sync::OnceLock;

use ratatui::style::Color;

/// Semantic color tokens for the whole UI; rendering reads these instead of
/// hardcoded `Color::` literals so a theme swap restyles every panel at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub button: Color,
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
}

impl Theme {
    /// The palette shipped before theming existed; must stay byte-identical
    /// to the colors previously hardcoded in `ui.rs`.
    pub const fn classic() -> Self {
        Self {
            button: Color::White,
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
        }
    }

    /// Every token as `Color::Reset`, letting each user's terminal pick the
    /// actual colors (works with custom schemes and low-color terminals).
    pub const fn terminal() -> Self {
        Self {
            button: Color::Reset,
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
        }
    }

    /// Grayscale palette; shift tokens are pairwise-distinct because shift
    /// groups are distinguished by color alone during normal patching.
    pub const fn mono() -> Self {
        Self {
            button: Color::White,
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

static ACTIVE: OnceLock<Theme> = OnceLock::new();

/// The theme rendering must use. Defaults to `classic` until `init` runs.
pub fn active() -> &'static Theme {
    // Fall back without claiming the slot: startup calls `init` before any
    // rendering, and a later `init` must not be silently swallowed.
    ACTIVE.get().unwrap_or(&CLASSIC)
}

/// Installs the startup-selected theme. A second call is ignored because
/// rendering holds references into the first value.
pub fn init(theme: Theme) {
    let _ = ACTIVE.set(theme);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_matches_previous_hardcoded_colors() {
        let t = Theme::classic();
        assert_eq!(t.button, Color::White);
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
    }

    #[test]
    fn terminal_is_all_reset() {
        let t = Theme::terminal();
        for color in [
            t.button,
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
        ] {
            assert_eq!(color, Color::Reset);
        }
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
        assert_eq!(*active(), Theme::classic());
        init(Theme::terminal());
        assert_eq!(*active(), Theme::terminal());
    }
}
