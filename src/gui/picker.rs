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
//! The shell (main.rs / task 2.6) builds [`PickerSpec`] from `App` state:
//! `picker_entries` (already ordered by `App::refresh_picker_entries`), the
//! favourite set, and `picker_index`. Tests build specs directly.

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::theme::Color;

/// One picker row for painting: the display label (★-prefixed for favourites,
/// `..` sentinel untouched), its favourite/dir flags, and whether it is the
/// current cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PickerRow {
    pub label: String,
    pub is_favourite: bool,
    pub is_dir: bool,
    pub is_parent: bool,
}

/// The fully-resolved picker payload for one frame. Pure data: the shell
/// builds it from `App`, the tests build it directly; [`paint_picker`] only
/// draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PickerSpec {
    pub title: String,
    pub picker_dir: String,
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
    let content_top = if spec.picker_dir.is_empty() {
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
}
