//! File picker + favourites surface (task 2.4): the file picker overlay
//! ported from `ui.rs::render_picker` to an egui draw routine.
//!
//! The surface follows the other gui surfaces: a fully-resolved pure-data
//! payload ([`PickerSpec`]) built each frame, a draw routine ([`paint_picker`])
//! that only paints it, and pure entry-ordering helpers
//! ([`ordered_entries`], [`favourited_label`]) that keep the
//! favourites-pinned, dirs-first, separator logic testable headless. Input is
//! reported back as a [`PickerFrame`] (hovered/clicked entry) so the window
//! loop applies the same semantics the terminal handler used: `j`/`k` move the
//! cursor, `Enter` opens, `f` toggles favourite.
//!
//! The shell (main.rs / task 2.6) builds [`PickerSpec`] from `App` state
//! (`picker_entries`, already ordered by `App::refresh_picker_entries`, the
//! favourite set, and `picker_index`) via [`picker_spec`]. Tests build specs
//! directly.

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::app::{is_picker_parent_entry, App};
use crate::theme::Color;

/// One picker row for painting: the display label (★-prefixed for favourites,
/// `..` sentinel untouched), its favourite/dir flags, and whether it is the
/// current cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickerRow {
    pub label: String,
    pub is_favourite: bool,
    pub is_dir: bool,
    pub is_parent: bool,
}

/// The fully-resolved picker payload for one frame. Pure data: the shell
/// builds it from `App`, the tests build it directly; [`paint_picker`] only
/// draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickerSpec {
    pub title: String,
    pub picker_dir: String,
    /// Latching file filter string (bead t9g). `None` renders nothing and
    /// leaves the layout identical to the pre-filter picker; `Some(s)` draws
    /// the `filter: <string>` subtitle line under the dir line.
    pub filter: Option<String>,
    pub rows: Vec<PickerRow>,
    pub selected: usize,
    pub has_favourites: bool,
    pub fav_count: usize,
}

/// The frame's picker input report, applied by the loop to `App`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PickerFrame {
    /// Row index under the pointer.
    pub hovered: Option<usize>,
    /// Row index primary-clicked this frame.
    pub clicked: Option<usize>,
}

/// Build the display label for a picker entry, mirroring
/// `App::picker_entry_label`: favourited files keep their leaf name with
/// `★ `, favourited directories add a trailing `/`, the parent sentinel `..`
/// is never favourited.
pub(super) fn favourited_label(
    raw: &str,
    is_favourite: bool,
    is_dir: bool,
    is_parent: bool,
) -> String {
    if is_parent {
        return "..".to_string();
    }
    if !is_favourite {
        return raw.to_string();
    }
    if is_dir {
        format!("★ {}/", raw)
    } else {
        format!("★ {}", raw)
    }
}

/// Order raw entries into picker rows: favourites (as already ordered by
/// `App::refresh_picker_entries` – caller provides them first) keep their
/// order, then the separator is implied by `fav_count`, then the directory
/// listing stays as provided. This helper only labels; the actual ordering
/// (dirs first, then .ini, then others) is performed by `App::refresh_picker_entries`
/// and verified by its unit tests. Here we only verify that the spec preserves
/// the fav-pinned ordering and that the painted output inserts the separator
/// at the right place.
pub(super) fn ordered_entries(
    favs: Vec<(String, bool)>,
    listing: Vec<(String, bool, bool)>,
) -> Vec<PickerRow> {
    let mut rows = Vec::new();
    for (label, is_dir) in favs {
        rows.push(PickerRow {
            label: favourited_label(&label, true, is_dir, false),
            is_favourite: true,
            is_dir,
            is_parent: false,
        });
    }
    for (label, is_dir, is_parent) in listing {
        if is_parent {
            rows.push(PickerRow {
                label: "..".to_string(),
                is_favourite: false,
                is_dir: false,
                is_parent: true,
            });
        } else {
            rows.push(PickerRow {
                label,
                is_favourite: false,
                is_dir,
                is_parent: false,
            });
        }
    }
    rows
}

/// Build the fully-resolved picker payload for one frame (port of
/// `ui.rs::render_picker`). The rows come from the already-ordered
/// `app.picker_entries` (favourites pinned first by
/// `App::refresh_picker_entries`), labelled via [`ordered_entries`] so the
/// favourite colour/slash rules stay in one place; `fav_count` marks the
/// separator position exactly as the terminal did. The dispatch task calls
/// this while the picker is open and hands the spec to [`paint_picker`].
pub(crate) fn picker_spec(app: &App) -> PickerSpec {
    let favs: Vec<(String, bool)> = app
        .picker_entries_with_favourites()
        .iter()
        .map(|p| {
            (
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                p.metadata().is_ok_and(|m| m.is_dir()),
            )
        })
        .collect();
    let fav_count = favs.len();
    let has_favourites = fav_count > 0 && !app.picker_entries.is_empty();
    // Everything after the pinned favourites: the `..` sentinel plus the
    // directory listing. `refresh_picker_entries` deduplicates favourites out
    // of the listing, so the front `fav_count` entries are exactly `favs`.
    let listing: Vec<(String, bool, bool)> = app
        .picker_entries
        .iter()
        .skip(fav_count.min(app.picker_entries.len()))
        .map(|p| {
            (
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                p.metadata().is_ok_and(|m| m.is_dir()),
                is_picker_parent_entry(p),
            )
        })
        .collect();
    PickerSpec {
        title: " File Picker ".to_string(),
        picker_dir: app.picker_dir.display().to_string(),
        filter: if app.picker_filter_active {
            Some(app.picker_filter.clone())
        } else {
            None
        },
        rows: ordered_entries(favs, listing),
        selected: app.picker_index,
        has_favourites,
        fav_count,
    }
}

/// Paint the file picker overlay. `None` draws nothing. Returns the frame's
/// hover/click report for the loop to apply.
pub(super) fn paint_picker(
    painter: &Painter,
    canvas: Vec2,
    ctx: &Context,
    spec: Option<&PickerSpec>,
) -> PickerFrame {
    let Some(spec) = spec else {
        return PickerFrame::default();
    };
    let t = crate::theme::active();

    // Centered overlay: 70% width, 50% height, at least 40×20, centered.
    let w = (canvas.x * 0.70).clamp(40.0, canvas.x - 4.0).max(40.0);
    let h = (canvas.y * 0.50).clamp(20.0, canvas.y - 4.0).max(20.0);
    let picker_rect = Rect::from_min_size(
        Pos2::new((canvas.x - w) / 2.0, (canvas.y - h) / 2.0),
        Vec2::new(w, h),
    );

    // Background + border (muted bg, accent border, title).
    painter.rect_filled(picker_rect, 0.0, rgb(t.muted));
    painter.rect(
        picker_rect,
        0.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, rgb(t.accent)),
        egui::StrokeKind::Inside,
    );
    if !spec.title.is_empty() {
        painter.text(
            picker_rect.min + egui::vec2(6.0, 3.0),
            egui::Align2::LEFT_TOP,
            &spec.title,
            egui::FontId::proportional(12.0),
            rgb(t.accent),
        );
    }
    // Picker directory subtitle under title, if provided.
    if !spec.picker_dir.is_empty() && picker_rect.height() >= 24.0 {
        painter.text(
            picker_rect.min + egui::vec2(6.0, 18.0),
            egui::Align2::LEFT_TOP,
            &spec.picker_dir,
            egui::FontId::proportional(10.0),
            rgb(t.muted),
        );
    }

    let inner = picker_rect.shrink(4.0);
    let content_top = if let Some(filter) = &spec.filter {
        // Filter line under the dir subtitle (or in its slot when there is
        // no dir line). Row content starts below it only in this branch, so
        // `filter: None` keeps the pre-filter layout byte-identical.
        let filter_y = if spec.picker_dir.is_empty() {
            18.0
        } else {
            31.0
        };
        painter.text(
            picker_rect.min + egui::vec2(6.0, filter_y),
            egui::Align2::LEFT_TOP,
            format!("filter: {filter}"),
            egui::FontId::proportional(10.0),
            rgb(t.muted),
        );
        inner.min.y + (filter_y - 4.0) + 16.0
    } else if spec.picker_dir.is_empty() {
        inner.min.y + 16.0
    } else {
        inner.min.y + 30.0
    };
    let mut y = content_top;
    let row_h = 16.0;
    let mut row_rects: Vec<(usize, Rect)> = Vec::new();

    for (idx, row) in spec.rows.iter().enumerate() {
        // Separator between favourites and listing.
        if spec.has_favourites && idx == spec.fav_count && y + row_h <= inner.max.y {
            painter.text(
                Pos2::new(inner.min.x + 4.0, y),
                egui::Align2::LEFT_TOP,
                "── favourites ──",
                egui::FontId::proportional(10.0),
                rgb(t.muted),
            );
            y += row_h;
        }
        if y + row_h > inner.max.y {
            break;
        }
        let is_selected = idx == spec.selected;
        let prefix = if is_selected { "▶ " } else { "  " };
        let text = format!("{}{}", prefix, row.label);

        let bg = if is_selected {
            let mut c = rgb(t.muted);
            c = egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 90);
            Some(c)
        } else {
            None
        };
        let rect = Rect::from_min_size(
            Pos2::new(inner.min.x + 2.0, y),
            Vec2::new(inner.width() - 4.0, row_h),
        );
        // Background for selected row.
        if let Some(c) = bg {
            painter.rect_filled(rect, 0.0, c);
        }
        let base_fg = if row.is_favourite {
            if row.is_dir {
                t.picker_fav_dir
            } else {
                t.picker_fav_file
            }
        } else {
            t.text
        };
        let fg = rgb(base_fg);
        // Selected keeps bold emphasis; favourite colour stays.
        let font = egui::FontId::proportional(11.0);
        painter.text(
            Pos2::new(inner.min.x + 6.0, y),
            egui::Align2::LEFT_TOP,
            text,
            font,
            fg,
        );
        row_rects.push((idx, rect));
        y += row_h;
    }

    ctx.input(|i| picker_frame(i, &row_rects, spec.rows.len()))
}

fn picker_frame(i: &egui::InputState, row_rects: &[(usize, Rect)], _total: usize) -> PickerFrame {
    let mut frame = PickerFrame::default();
    let Some(pos) = i.pointer.latest_pos() else {
        return frame;
    };
    for (idx, rect) in row_rects {
        if rect.contains(pos) {
            frame.hovered = Some(*idx);
            break;
        }
    }
    if i.pointer.primary_clicked() {
        frame.clicked = frame.hovered;
    }
    frame
}

fn rgb(color: Color) -> egui::Color32 {
    crate::theme::active().egui_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Vec2};

    fn spec_with_favourites() -> PickerSpec {
        // Favourites pinned top: a dir and a .ini, then separator, then listing.
        let favs = vec![
            ("mydir".to_string(), true),
            ("patch.ini".to_string(), false),
        ];
        let listing = vec![
            ("..".to_string(), false, true),
            ("subdir".to_string(), true, false),
            ("other.ini".to_string(), false, false),
            ("readme.txt".to_string(), false, false),
        ];
        let rows = ordered_entries(favs, listing);
        PickerSpec {
            title: " File Picker ".into(),
            picker_dir: "/tmp".into(),
            filter: None,
            rows,
            selected: 0,
            has_favourites: true,
            fav_count: 2,
        }
    }

    fn painted_labels(spec: &PickerSpec) -> Vec<String> {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_picker(ui.painter(), ui.max_rect().size(), ui.ctx(), Some(spec));
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
        full_output.textures_delta.clear();
        labels
    }

    #[test]
    fn favourited_label_adds_star_and_slash() {
        assert_eq!(favourited_label("mydir", true, true, false), "★ mydir/");
        assert_eq!(
            favourited_label("patch.ini", true, false, false),
            "★ patch.ini"
        );
        assert_eq!(favourited_label("..", true, false, true), "..");
        assert_eq!(
            favourited_label("other.ini", false, false, false),
            "other.ini"
        );
    }

    #[test]
    fn ordered_entries_preserves_fav_order_and_labels() {
        let favs = vec![("b.ini".to_string(), false), ("adir".to_string(), true)];
        let listing = vec![("z.txt".to_string(), false, false)];
        let rows = ordered_entries(favs, listing);
        assert_eq!(rows[0].label, "★ b.ini");
        assert_eq!(rows[1].label, "★ adir/");
        assert_eq!(rows[2].label, "z.txt");
        assert!(rows[0].is_favourite && !rows[2].is_favourite);
    }

    #[test]
    fn paint_picker_draws_title_and_favourites_with_separator() {
        let labels = painted_labels(&spec_with_favourites());
        assert!(
            labels.iter().any(|l| l.contains("File Picker")),
            "title: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("★ mydir/")),
            "fav dir: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("★ patch.ini")),
            "fav file: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("── favourites ──")),
            "separator: {labels:?}"
        );
        // Listing after separator still rendered.
        assert!(
            labels.iter().any(|l| l.contains("subdir")),
            "listing: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("other.ini")),
            "listing ini: {labels:?}"
        );
        // Selected row has ▶ prefix.
        assert!(
            labels.iter().any(|l| l.contains("▶")),
            "selected marker: {labels:?}"
        );
    }

    #[test]
    fn paint_picker_respects_entry_ordering_dirs_first() {
        // App ordering is dirs, then .ini, then others – the spec should preserve that.
        let favs = vec![];
        let listing = vec![
            ("..".to_string(), false, true),
            ("adir".to_string(), true, false),
            ("b.ini".to_string(), false, false),
            ("notes.txt".to_string(), false, false),
        ];
        let rows = ordered_entries(favs, listing);
        let spec = PickerSpec {
            title: " File Picker ".into(),
            picker_dir: "".into(),
            filter: None,
            rows,
            selected: 1,
            has_favourites: false,
            fav_count: 0,
        };
        let labels = painted_labels(&spec);
        // Order in painted output should be .., adir, b.ini, notes.txt (dirs first).
        let pos_dir = labels.iter().position(|l| l.contains("adir")).unwrap();
        let pos_ini = labels.iter().position(|l| l.contains("b.ini")).unwrap();
        let pos_txt = labels.iter().position(|l| l.contains("notes.txt")).unwrap();
        assert!(
            pos_dir < pos_ini && pos_ini < pos_txt,
            "dirs first: {labels:?}"
        );
    }

    #[test]
    fn picker_frame_reports_hover_and_click() {
        let spec = spec_with_favourites();
        let ctx = egui::Context::default();
        // Seed pointer inside second row (y ~ 60).
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(400.0, 300.0),
            )),
            events: vec![
                egui::Event::PointerMoved(Pos2::new(150.0, 132.0)),
                egui::Event::PointerButton {
                    pos: Pos2::new(150.0, 132.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerButton {
                    pos: Pos2::new(150.0, 132.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            ..Default::default()
        };
        let mut frame = PickerFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = paint_picker(ui.painter(), ui.max_rect().size(), ui.ctx(), Some(&spec));
        });
        full_output.textures_delta.clear();
        assert!(frame.hovered.is_some(), "hovered: {frame:?}");
        assert_eq!(frame.clicked, frame.hovered);
    }

    #[test]
    fn paint_none_spec_draws_nothing() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let mut frame = PickerFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = paint_picker(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
        });
        full_output.textures_delta.clear();
        assert_eq!(frame, PickerFrame::default());
        assert!(full_output.shapes.is_empty());
    }

    #[test]
    fn paint_picker_draws_filter_line_when_set() {
        let mut spec = spec_with_favourites();
        spec.filter = Some("abba".to_string());
        let labels = painted_labels(&spec);
        assert!(
            labels.iter().any(|l| l == "filter: abba"),
            "filter line: {labels:?}"
        );
    }

    #[test]
    fn paint_picker_filter_none_renders_no_filter_label() {
        let spec = spec_with_favourites();
        let labels = painted_labels(&spec);
        assert!(
            !labels.iter().any(|l| l.starts_with("filter:")),
            "no filter line when None: {labels:?}"
        );
    }

    #[test]
    fn paint_picker_active_empty_filter_still_shows_latch() {
        let mut spec = spec_with_favourites();
        spec.filter = Some(String::new());
        let labels = painted_labels(&spec);
        assert!(
            labels.iter().any(|l| l == "filter: "),
            "empty active filter renders `filter: `: {labels:?}"
        );
    }

    #[test]
    fn picker_spec_carries_app_filter_state() {
        use std::path::PathBuf;
        let mut app = App::new();
        app.favorites = crate::favorites::FavoritesStore::default();
        app.picker_entries = vec![PathBuf::from("/tmp/b.ini"), PathBuf::from("..")];
        app.picker_index = 1;
        assert_eq!(picker_spec(&app).filter, None);
        app.picker_filter_active = true;
        app.picker_filter = "abba".to_string();
        assert_eq!(picker_spec(&app).filter, Some("abba".to_string()));
    }

    #[test]
    fn picker_spec_builds_rows_from_app() {
        use std::path::PathBuf;
        let mut app = App::new();
        // App::new() loads the real favourites store; empty it so the
        // favourites section and separator are absent.
        app.favorites = crate::favorites::FavoritesStore::default();
        app.picker_entries = vec![PathBuf::from("/tmp/b.ini"), PathBuf::from("..")];
        app.picker_index = 1;
        let spec = picker_spec(&app);
        assert_eq!(spec.title, " File Picker ");
        assert_eq!(spec.picker_dir, app.picker_dir.display().to_string());
        assert!(!spec.has_favourites);
        assert_eq!(spec.fav_count, 0);
        assert_eq!(spec.rows.len(), 2);
        assert_eq!(spec.rows[0].label, "b.ini");
        assert!(!spec.rows[0].is_favourite);
        assert_eq!(spec.rows[1].label, "..");
        assert!(spec.rows[1].is_parent);
        assert_eq!(spec.selected, 1);
    }
}
