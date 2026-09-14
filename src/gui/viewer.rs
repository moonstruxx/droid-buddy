//! Source viewer surface (task 2.3): the raw/prettified source pane with
//! sidebar and minimap, ported from `ui.rs::render_source_pane` +
//! `render_source_sidebar` + `render_source_content` + `render_minimap` +
//! `render_viewer_status`.
//!
//! Like the other surfaces it draws a fully-resolved pure-data payload
//! ([`ViewerSpec`]) and reports input back as a [`ViewerFrame`]. The
//! highlight/geometry logic that the terminal kept inside the renderer is
//! ported here as pure helpers so it tests headless: [`raw_line_kinds`] +
//! [`line_fragments`] (occurrence/modifier highlighting over raw lines),
//! [`entry_value_fragments`] + [`prettified_box_lines`] (prettified boxes),
//! [`minimap_rows`] (map rows + viewport indicator), and [`pane_split`] (the
//! sidebar|content|minimap width negotiation). The shell builds the
//! [`ViewerSpec`] from `App` state (patch lines, `occurrence_cursor`,
//! `source_scroll`, selection); tests build it directly.

use std::collections::{HashMap, HashSet};

use egui::{Context, Painter, Pos2, Rect, Vec2};

use crate::app::{App, FocusSlot, SourceViewMode, ViewType};
use crate::patch::{ModifierAffect, NodeId, Patch, Span};
use crate::theme::Color;

/// The per-column highlight kind of a raw source line (port of
/// `ui.rs::HighlightKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HighlightKind {
    None,
    OccCurrent,
    OccOther,
    ModCyan,
    ModMagenta,
}

/// One styled run of a source line, in byte offsets.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fragment {
    /// Byte range `(start, end)` into the line text.
    pub range: (usize, usize),
    pub color: Color,
    pub bold: bool,
    pub reversed: bool,
    pub underlined: bool,
}

/// One rendered source line: the full text plus styled fragments (the
/// un-fragmented spans read as plain theme-text runs).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineSpec {
    pub text: String,
    pub fragments: Vec<Fragment>,
}

/// The circuits sidebar column.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SidebarSpec {
    /// Display names (disambiguated), in section order.
    pub names: Vec<String>,
    /// Index of the selected circuit entry.
    pub selected: Option<usize>,
}

/// One minimap row: its glyph, color, and whether the viewport indicator
/// reverses it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MinimapRow {
    pub ch: char,
    pub color: Color,
    pub reversed: bool,
}

/// The minimap column.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MinimapSpec {
    pub rows: Vec<MinimapRow>,
}

/// One status-bar fragment (hint text or the transient message).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StatusFragment {
    pub text: String,
    pub color: Color,
    pub bold: bool,
}

/// The fully-resolved viewer payload for one frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ViewerSpec {
    /// Pane title: " Source [raw] " / " Source [prettified] ".
    pub title: String,
    pub focused: bool,
    /// Content lines (raw or prettified), not yet scrolled.
    pub lines: Vec<LineSpec>,
    /// Line offset of the first visible content line.
    pub scroll: usize,
    /// Centered message when the content is empty ("No patch loaded", ...).
    pub empty_message: Option<String>,
    /// `None` hides the circuits sidebar.
    pub sidebar: Option<SidebarSpec>,
    /// `None` hides the minimap.
    pub minimap: Option<MinimapSpec>,
    /// Status-bar fragments drawn at the bottom (hints + transient message).
    pub status: Vec<StatusFragment>,
}

/// The frame's viewer input report: wheel scroll over the content column.
/// The key bindings (j/k, Up/Down, Home/End, t, Esc, Tab, `[`/`]`) stay in
/// the loop's handler mapping (task 2.6), which reads the same frame state.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ViewerFrame {
    /// Wheel delta over the content column (positive = scroll down).
    pub scroll_delta: f32,
}

/// Paint the source viewer. `None` draws nothing. Returns the frame's scroll
/// report for the loop to apply.
pub(super) fn paint_viewer(
    painter: &Painter,
    canvas: Vec2,
    ctx: &Context,
    spec: Option<&ViewerSpec>,
) -> ViewerFrame {
    let Some(spec) = spec else {
        return ViewerFrame::default();
    };
    let t = crate::theme::active();

    // Status strip at the bottom: hints (viewer-key accent) + transient
    // message, on the status background.
    let status_h = 22.0;
    let status_rect = Rect::from_min_size(
        Pos2::new(0.0, canvas.y - status_h),
        Vec2::new(canvas.x, status_h),
    );
    painter.rect_filled(status_rect, 0.0, rgb(t.status_bg));
    let key = rgb(t.viewer_key);
    let text = rgb(t.text);
    let mut x = 6.0;
    for f in &spec.status {
        let font = egui::FontId::proportional(11.0);
        painter.text(
            Pos2::new(x, status_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &f.text,
            font.clone(),
            if f.bold { text } else { key },
        );
        x += f.text.chars().count() as f32 * font.size * 0.55 + 2.0;
    }

    // The pane above the status strip: border + mode title.
    let pane = Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new(canvas.x, (canvas.y - status_h).max(1.0)),
    );
    let border = if spec.focused {
        rgb(t.focus_border)
    } else {
        rgb(t.muted)
    };
    painter.rect(
        pane,
        0.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(if spec.focused { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Inside,
    );
    if !spec.title.is_empty() {
        painter.text(
            pane.min + egui::vec2(6.0, 3.0),
            egui::Align2::LEFT_TOP,
            &spec.title,
            egui::FontId::proportional(12.0),
            border,
        );
    }

    let inner = pane.shrink(4.0);
    let (sidebar_w, content_w, minimap_w) = pane_split(
        inner.width(),
        spec.sidebar.is_some(),
        spec.minimap.is_some(),
        80.0,
    )
    .unwrap_or((0.0, inner.width(), 0.0));
    let mut content_rect = Rect::from_min_size(
        Pos2::new(inner.min.x + sidebar_w, inner.min.y),
        Vec2::new(content_w, inner.height()),
    );
    if let Some(sidebar) = &spec.sidebar {
        let srect = Rect::from_min_size(inner.min, Vec2::new(sidebar_w, inner.height()));
        painter.rect(
            srect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, rgb(t.accent)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            srect.min + egui::vec2(5.0, 3.0),
            egui::Align2::LEFT_TOP,
            " Circuits ",
            egui::FontId::proportional(11.0),
            rgb(t.accent),
        );
        let mut y = srect.min.y + 22.0;
        let row_h = 16.0;
        for (i, name) in sidebar.names.iter().enumerate() {
            if y + row_h > srect.max.y {
                break;
            }
            let selected = Some(i) == sidebar.selected;
            if selected {
                let mut bg = rgb(t.muted);
                bg = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 120);
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(srect.min.x + 2.0, y),
                        Vec2::new(sidebar_w - 4.0, row_h),
                    ),
                    0.0,
                    bg,
                );
            }
            painter.text(
                Pos2::new(srect.min.x + 5.0, y),
                egui::Align2::LEFT_TOP,
                name,
                egui::FontId::proportional(11.0),
                rgb(t.text),
            );
            y += row_h;
        }
    }
    if let Some(minimap) = &spec.minimap {
        let mrect = Rect::from_min_size(
            Pos2::new(inner.max.x - minimap_w, inner.min.y),
            Vec2::new(minimap_w, inner.height()),
        );
        painter.rect(
            mrect,
            0.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, rgb(t.muted)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            mrect.min + egui::vec2(3.0, 2.0),
            egui::Align2::LEFT_TOP,
            " Map ",
            egui::FontId::proportional(9.0),
            rgb(t.muted),
        );
        let row_h = (mrect.height() / minimap.rows.len().max(1) as f32).clamp(1.0, 3.0);
        for (row_idx, row) in minimap.rows.iter().enumerate() {
            let y = mrect.min.y + row_idx as f32 * row_h + row_h * 0.5;
            if row.reversed {
                let mut bg = rgb(t.muted);
                bg = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 160);
                painter.rect_filled(
                    Rect::from_min_size(mrect.min, Vec2::new(minimap_w, mrect.height())),
                    0.0,
                    bg,
                );
            }
            painter.text(
                Pos2::new(mrect.min.x + 2.0, y),
                egui::Align2::LEFT_CENTER,
                row.ch.to_string(),
                egui::FontId::monospace(row_h.max(4.0)),
                if row.reversed {
                    rgb(t.text)
                } else {
                    rgb(row.color)
                },
            );
        }
        content_rect = Rect::from_min_size(
            Pos2::new(content_rect.min.x, content_rect.min.y),
            Vec2::new(content_rect.width() - minimap_w, content_rect.height()),
        );
    }

    // Content column: scrolled highlighted lines, clipped to the column.
    if let Some(message) = &spec.empty_message {
        if spec.lines.is_empty() {
            painter.text(
                content_rect.center(),
                egui::Align2::CENTER_CENTER,
                message,
                egui::FontId::proportional(13.0),
                rgb(t.muted),
            );
        }
    }
    let clip = painter.with_clip_rect(content_rect);
    let font = egui::FontId::monospace(13.0);
    let line_h = font.size * 1.3;
    let mut y = content_rect.min.y;
    for line in spec.lines.iter().skip(spec.scroll) {
        if y + line_h > content_rect.max.y {
            break;
        }
        let mut x = content_rect.min.x;
        if line.fragments.is_empty() {
            clip.text(
                Pos2::new(x, y),
                egui::Align2::LEFT_TOP,
                &line.text,
                font.clone(),
                rgb(t.text),
            );
        } else {
            // Draw the styled fragments plus the unstyled gaps between them
            // (plain theme text), so highlighted lines keep their full text.
            let mut pos = 0usize;
            for f in &line.fragments {
                let start = f.range.0.min(line.text.len());
                if start > pos {
                    let gap = line.text.get(pos..start).unwrap_or("");
                    if !gap.is_empty() {
                        clip.text(
                            Pos2::new(x, y),
                            egui::Align2::LEFT_TOP,
                            gap,
                            font.clone(),
                            rgb(t.text),
                        );
                        x += gap.chars().count() as f32 * font.size * 0.6;
                    }
                }
                let frag = line
                    .text
                    .get(f.range.0..f.range.1.min(line.text.len()))
                    .unwrap_or("");
                if !frag.is_empty() {
                    // Reversed fragments render in the theme text color over
                    // the muted backdrop (the terminal's REVERSED emphasis).
                    let color = if f.reversed {
                        rgb(t.text)
                    } else {
                        rgb(f.color)
                    };
                    clip.text(
                        Pos2::new(x, y),
                        egui::Align2::LEFT_TOP,
                        frag,
                        font.clone(),
                        color,
                    );
                    x += frag.chars().count() as f32 * font.size * 0.6;
                }
                pos = f.range.1.max(pos);
            }
            if pos < line.text.len() {
                let gap = line.text.get(pos..).unwrap_or("");
                if !gap.is_empty() {
                    clip.text(
                        Pos2::new(x, y),
                        egui::Align2::LEFT_TOP,
                        gap,
                        font.clone(),
                        rgb(t.text),
                    );
                }
            }
        }
        y += line_h;
    }

    // Wheel scroll over the content column.
    ctx.input(|i| viewer_frame(i, content_rect))
}

/// The viewer's wheel-scroll report: the scroll delta when the pointer is
/// over the content column. Pure so input tests run without a window.
pub(super) fn viewer_frame(i: &egui::InputState, content_rect: Rect) -> ViewerFrame {
    let mut frame = ViewerFrame::default();
    let over = i
        .pointer
        .latest_pos()
        .is_some_and(|p| content_rect.contains(p));
    if over && i.smooth_scroll_delta.y.abs() > f32::EPSILON {
        frame.scroll_delta = i.smooth_scroll_delta.y;
    }
    frame
}

/// The sidebar|content|minimap width negotiation (port of the
/// `render_source_pane` chunking): the sidebar takes ~1/5 (clamped), the
/// minimap a fixed sliver, and the content the rest, hiding the minimap and
/// then the sidebar when the content would shrink below `min_content`.
/// Returns `None` when nothing fits.
pub(super) fn pane_split(
    inner_w: f32,
    want_sidebar: bool,
    want_minimap: bool,
    min_content: f32,
) -> Option<(f32, f32, f32)> {
    if inner_w <= 0.0 {
        return None;
    }
    let mut sidebar_w = if want_sidebar && inner_w > 120.0 {
        (inner_w / 5.0).clamp(60.0, inner_w - 60.0)
    } else {
        0.0
    };
    let minimap_w = if want_minimap { 26.0 } else { 0.0 };
    let mut content_w = inner_w - sidebar_w - minimap_w;
    if want_minimap && content_w < min_content {
        content_w += minimap_w;
        if content_w < min_content && want_sidebar {
            content_w += sidebar_w;
            if content_w < min_content {
                return None;
            }
            return Some((0.0, content_w, 0.0));
        }
        return Some((sidebar_w, content_w, 0.0));
    }
    if want_sidebar && content_w < min_content {
        content_w += sidebar_w;
        sidebar_w = 0.0;
        if content_w < min_content {
            return None;
        }
    }
    Some((sidebar_w, content_w, minimap_w))
}

/// Fill a raw line's per-byte highlight kinds from occurrence spans and
/// modifier affects (port of the per-line loop in
/// `ui.rs::build_raw_highlighted_lines`): occurrences mark their byte range
/// (the cursor's occurrence wins), modifiers override non-current
/// occurrences, and the current occurrence keeps top priority.
pub(super) fn raw_line_kinds(
    raw: &str,
    occ_spans: &[Span],
    mod_affects: &[ModifierAffect],
    current: Option<Span>,
) -> Vec<HighlightKind> {
    let mut kinds: Vec<HighlightKind> = vec![HighlightKind::None; raw.len()];
    for span in occ_spans {
        let is_current = Some(*span) == current;
        let kind = if is_current {
            HighlightKind::OccCurrent
        } else {
            HighlightKind::OccOther
        };
        let start = span.col_start.min(raw.len());
        let len = span.col_end.min(raw.len()).saturating_sub(start);
        for slot in kinds.iter_mut().skip(start).take(len) {
            if *slot == HighlightKind::None
                || (*slot == HighlightKind::OccOther && kind == HighlightKind::OccCurrent)
            {
                *slot = kind;
            }
        }
    }
    for affect in mod_affects {
        let kind = if affect.selectat.is_some() {
            HighlightKind::ModMagenta
        } else {
            HighlightKind::ModCyan
        };
        let start = affect.span.col_start.min(raw.len());
        let len = affect.span.col_end.min(raw.len()).saturating_sub(start);
        for slot in kinds.iter_mut().skip(start).take(len) {
            if *slot != HighlightKind::OccCurrent {
                *slot = kind;
            }
        }
    }
    kinds
}

/// Turn per-byte kinds into styled fragments (port of the span-run assembly
/// in `ui.rs::build_raw_highlighted_lines`).
pub(super) fn line_fragments(raw: &str, kinds: &[HighlightKind]) -> LineSpec {
    let t = crate::theme::active();
    let mut fragments = Vec::new();
    let mut start = 0usize;
    while start < raw.len() {
        let cur = kinds[start];
        let mut end = start + 1;
        while end < raw.len() && kinds[end] == cur {
            end += 1;
        }
        if cur != HighlightKind::None {
            let (color, bold, reversed, underlined) = match cur {
                HighlightKind::None => unreachable!(),
                HighlightKind::OccOther => (t.occurrence_highlight, true, false, false),
                HighlightKind::OccCurrent => (t.occurrence_highlight, true, true, false),
                HighlightKind::ModCyan => (t.modifier_boolean, true, false, true),
                HighlightKind::ModMagenta => (t.modifier_exact, true, false, true),
            };
            fragments.push(Fragment {
                range: (start, end),
                color,
                bold,
                reversed,
                underlined,
            });
        }
        start = end;
    }
    LineSpec {
        text: raw.to_string(),
        fragments,
    }
}

/// Token occurrences inside a prettified value (port of
/// `ui.rs::find_token_spans_in_value`): whole-word matches, not preceded by
/// an alphanumeric/underscore and not followed by one or a dot.
pub(super) fn token_spans_in_value(value: &str, target: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if target.is_empty() || value.is_empty() {
        return out;
    }
    let mut search_start = 0usize;
    while search_start <= value.len().saturating_sub(target.len()) {
        let Some(rel) = value[search_start..].find(target) else {
            break;
        };
        let s = search_start + rel;
        let e = s + target.len();
        let before_ok = if s == 0 {
            true
        } else {
            let c = value.as_bytes()[s - 1] as char;
            !(c.is_ascii_alphanumeric() || c == '_')
        };
        let after_ok = if e >= value.len() {
            true
        } else {
            let c = value.as_bytes()[e] as char;
            !(c.is_ascii_alphanumeric() || c == '_' || c == '.')
        };
        if before_ok && after_ok {
            out.push((s, e));
            search_start = e;
        } else {
            search_start = s + 1;
        }
    }
    out
}

/// One prettified circuit entry: the key plus the already-resolved value
/// fragments.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct EntrySpec {
    pub key: String,
    pub value: String,
    pub fragments: Vec<Fragment>,
}

/// The styled value fragments of a prettified entry (port of the value-span
/// branch in `ui.rs::build_prettified_highlighted_lines`): a modifier source
/// value renders in the modifier token (exact vs boolean), otherwise token
/// occurrences highlight in the occurrence token and the rest in plain text.
pub(super) fn entry_value_fragments(
    value: &str,
    selected: Option<&str>,
    mods: &[ModifierAffect],
) -> Vec<Fragment> {
    let t = crate::theme::active();
    let Some(token) = selected else {
        return vec![Fragment {
            range: (0, value.len()),
            color: t.text,
            bold: false,
            reversed: false,
            underlined: false,
        }];
    };
    let is_modifier_value = mods.iter().any(|e| e.source == value.trim());
    if is_modifier_value {
        let is_exact = mods
            .iter()
            .any(|e| e.source == value.trim() && e.selectat.is_some());
        return vec![Fragment {
            range: (0, value.len()),
            color: if is_exact {
                t.modifier_exact
            } else {
                t.modifier_boolean
            },
            bold: true,
            reversed: false,
            underlined: true,
        }];
    }
    let ranges = token_spans_in_value(value, token);
    if ranges.is_empty() {
        return vec![Fragment {
            range: (0, value.len()),
            color: t.text,
            bold: false,
            reversed: false,
            underlined: false,
        }];
    }
    let mut out = Vec::new();
    let mut last = 0usize;
    for (s, e) in ranges {
        if s > last {
            out.push(Fragment {
                range: (last, s),
                color: t.text,
                bold: false,
                reversed: false,
                underlined: false,
            });
        }
        out.push(Fragment {
            range: (s, e),
            color: t.occurrence_highlight,
            bold: true,
            reversed: true,
            underlined: false,
        });
        last = e;
    }
    if last < value.len() {
        out.push(Fragment {
            range: (last, value.len()),
            color: t.text,
            bold: false,
            reversed: false,
            underlined: false,
        });
    }
    out
}

/// Assemble one prettified circuit box (port of the box builder in
/// `ui.rs::build_prettified_highlighted_lines`): a framed title header, the
/// `key = value` entry lines aligned to a uniform interior width, and the
/// closing footer.
pub(super) fn prettified_box_lines(
    name: &str,
    color: Color,
    entries: &[EntrySpec],
) -> Vec<LineSpec> {
    let header_text = format!("\u{2500} {name} \u{2500}");
    let mut w = header_text.chars().count();
    for entry in entries {
        let text = format!("{} = {}", entry.key, entry.value);
        w = w.max(text.chars().count());
    }
    let mut lines = Vec::new();

    let top_pad = (w + 2).saturating_sub(header_text.chars().count());
    lines.push(LineSpec {
        text: format!(
            "\u{250C}\u{2500} {name} \u{2500}{}\u{2510}",
            "\u{2500}".repeat(top_pad)
        ),
        fragments: vec![Fragment {
            range: (0, 1),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        }],
    });

    for entry in entries {
        // "│ " + "key = value" padded to `w` + " │", all fragments positioned
        // over the real content so painted labels carry the entry text.
        let content = format!("{} = {}", entry.key, entry.value);
        let pad = w.saturating_sub(content.chars().count());
        let line_text = format!("\u{2502} {}{}\u{2502}", content, " ".repeat(pad));
        let mut frags = Vec::new();
        frags.push(Fragment {
            range: (0, 1),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        });
        frags.push(Fragment {
            range: (1, 2),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        });
        let key_start = 2;
        frags.push(Fragment {
            range: (key_start, key_start + entry.key.len()),
            color: crate::theme::active().viewer_key,
            bold: false,
            reversed: false,
            underlined: false,
        });
        let sep_start = key_start + entry.key.len();
        frags.push(Fragment {
            range: (sep_start, sep_start + 3),
            color: crate::theme::active().text,
            bold: false,
            reversed: false,
            underlined: false,
        });
        let value_start = sep_start + 3;
        for f in &entry.fragments {
            frags.push(Fragment {
                range: (value_start + f.range.0, value_start + f.range.1),
                color: f.color,
                bold: f.bold,
                reversed: f.reversed,
                underlined: f.underlined,
            });
        }
        let pad_start = value_start + entry.value.len();
        frags.push(Fragment {
            range: (pad_start, pad_start + pad),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        });
        let tail = pad_start + pad;
        frags.push(Fragment {
            range: (tail, tail + 1),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        });
        frags.push(Fragment {
            range: (tail + 1, tail + 2),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        });
        lines.push(LineSpec {
            text: line_text,
            fragments: frags,
        });
    }

    lines.push(LineSpec {
        text: format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(w + 2)),
        fragments: vec![Fragment {
            range: (0, 1),
            color,
            bold: false,
            reversed: false,
            underlined: false,
        }],
    });
    lines
}

/// The semantic color for a component-kind token (port of
/// `ui.rs::kind_token_color`): the declared/name token maps to the matching
/// theme token, anything else to the accent.
pub(super) fn kind_token_color(token: &str) -> Color {
    let t = crate::theme::active();
    match token {
        "button" | "switch" | "notebuttons" | "notobuttons" => t.button,
        "pot" | "encoder" | "faderbank" => t.knob,
        "cvin" | "cv_in" => t.cv_in,
        "cvout" | "cv_out" => t.cv_out,
        "led" => t.led,
        _ => t.accent,
    }
}

/// One minimap row's kind: which markers the row's line range contains and
/// whether the viewport indicator covers it (port of the row loop in
/// `ui.rs::render_minimap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowKind {
    Empty,
    Occ,
    Mod,
    Combined,
}

fn minimap_row_kind(
    total_lines: usize,
    inner_height: usize,
    row: usize,
    occ: &HashSet<usize>,
    exact: &HashSet<usize>,
    boolean: &HashSet<usize>,
) -> (RowKind, Color) {
    let t = crate::theme::active();
    let line_start = row * total_lines / inner_height.max(1);
    let line_end = ((row + 1) * total_lines / inner_height.max(1)).max(line_start + 1);
    let has_occ = (line_start..line_end).any(|l| occ.contains(&l));
    let has_mod = (line_start..line_end).any(|l| exact.contains(&l) || boolean.contains(&l));
    if has_mod && has_occ {
        (RowKind::Combined, t.minimap_combined)
    } else if has_mod {
        let is_exact = (line_start..line_end).any(|l| exact.contains(&l));
        (
            RowKind::Mod,
            if is_exact {
                t.minimap_modifier_exact
            } else {
                t.minimap_modifier_boolean
            },
        )
    } else if has_occ {
        (RowKind::Occ, t.minimap_occurrence)
    } else {
        (RowKind::Empty, t.muted)
    }
}

/// The minimap rows (port of `ui.rs::render_minimap`'s row builder):
/// occurrence/modifier marker glyphs per line range, with the viewport
/// indicator range reversed. `exact`/`boolean` hold the modifier lines
/// partitioned by whether the affecting entry has a `selectat` value.
pub(super) fn minimap_rows(
    total_lines: usize,
    inner_height: usize,
    occ: &HashSet<usize>,
    exact: &HashSet<usize>,
    boolean: &HashSet<usize>,
    scroll: usize,
) -> Vec<MinimapRow> {
    if inner_height == 0 || total_lines == 0 {
        return Vec::new();
    }
    let viewport_start = ((scroll * inner_height) / total_lines).min(inner_height);
    let viewport_end = (((scroll + inner_height) * inner_height) / total_lines).min(inner_height);
    let viewport_range = viewport_start..viewport_end.max(viewport_start + 1);
    let mut rows = Vec::with_capacity(inner_height);
    for row in 0..inner_height {
        let (kind, color) = minimap_row_kind(total_lines, inner_height, row, occ, exact, boolean);
        let ch = match kind {
            RowKind::Combined | RowKind::Occ => '\u{2588}',
            RowKind::Mod => '\u{2593}',
            RowKind::Empty => '\u{00B7}',
        };
        rows.push(MinimapRow {
            ch,
            color,
            reversed: viewport_range.contains(&row),
        });
    }
    rows
}

/// Nominal minimap height (rows) for the spec builder. The paint stretches
/// the rows across the real minimap column, so a fixed nominal keeps the
/// glyphs and the viewport band proportionally correct at any window size;
/// the terminal derived this from the pane height, which the spec builder
/// does not know (the dispatch task owns the published rects).
const MINIMAP_ROWS: usize = 256;

/// Build the fully-resolved viewer payload for one frame (port of
/// `ui.rs::render_source_pane` + `render_source_content` +
/// `render_source_sidebar` + `render_minimap` + `render_viewer_status`).
/// The dispatch task calls this with `&App` each frame and hands the spec to
/// [`paint_viewer`]; tests build specs directly.
pub(crate) fn viewer_spec(app: &App) -> ViewerSpec {
    let status = viewer_status(app);
    let title = match app.source_view_mode {
        SourceViewMode::Raw => " Source [raw] ".to_string(),
        SourceViewMode::Prettified => " Source [prettified] ".to_string(),
    };
    // Focus mirrors the handler's `sync_viewer_focus_from_tiles`: the viewer
    // slot holds tile focus.
    let focused = match app.tile_stack.focus {
        FocusSlot::Slot(i) => app.tile_stack.slots.get(i) == Some(&ViewType::SourceViewer),
        FocusSlot::Panels => false,
    };
    let Some(patch) = app.patch.as_ref() else {
        return ViewerSpec {
            title,
            focused,
            lines: vec![],
            scroll: 0,
            empty_message: Some("No patch loaded".to_string()),
            sidebar: None,
            minimap: None,
            status,
        };
    };
    let (lines, empty_message) = match app.source_view_mode {
        SourceViewMode::Raw => {
            if patch.raw_lines.is_empty() {
                (Vec::new(), Some("No patch loaded".to_string()))
            } else {
                (raw_highlighted_lines(patch, app), None)
            }
        }
        SourceViewMode::Prettified => {
            if patch.viewer_circuits().is_empty() {
                (Vec::new(), Some("No circuits in patch".to_string()))
            } else {
                (prettified_highlighted_lines(patch, app), None)
            }
        }
    };
    // A patch with neither sections nor raw lines showed the circuits message
    // before the per-mode empty checks, so it wins over raw mode's message.
    let (lines, empty_message) = if patch.sections.is_empty() && patch.raw_lines.is_empty() {
        (Vec::new(), Some("No circuits in patch".to_string()))
    } else {
        (lines, empty_message)
    };
    let sidebar = if patch.sections.is_empty() {
        None
    } else {
        Some(sidebar_spec(patch, app))
    };
    let minimap = if patch.raw_lines.is_empty() && patch.sections.is_empty() {
        None
    } else {
        Some(minimap_spec(patch, app))
    };
    ViewerSpec {
        title,
        focused,
        lines,
        scroll: app.source_scroll,
        empty_message,
        sidebar,
        minimap,
        status,
    }
}

/// Raw content lines with occurrence/modifier highlighting (port of
/// `ui.rs::build_raw_highlighted_lines`), reusing the per-line kind fill and
/// fragment helpers. Without a selected token the raw lines pass through
/// unhighlighted.
fn raw_highlighted_lines(patch: &Patch, app: &App) -> Vec<LineSpec> {
    let Some(token) = app.selected_component.as_deref() else {
        return patch
            .raw_lines
            .iter()
            .map(|text| LineSpec {
                text: text.clone(),
                fragments: vec![],
            })
            .collect();
    };
    let occ_spans = patch.occurrences_for(token);
    let mod_affects = patch.modifier_entries_for(token);
    let current = occ_spans.get(app.occurrence_cursor).copied();
    patch
        .raw_lines
        .iter()
        .enumerate()
        .map(|(line_idx, raw)| {
            // The helpers fill byte kinds from spans without line checks, so
            // filter each line's spans here (the terminal did the same in its
            // per-line loop).
            let occ: Vec<Span> = occ_spans
                .iter()
                .filter(|s| s.line == line_idx)
                .copied()
                .collect();
            let mods: Vec<ModifierAffect> = mod_affects
                .iter()
                .filter(|m| m.span.line == line_idx)
                .cloned()
                .collect();
            let kinds = raw_line_kinds(raw, &occ, &mods, current);
            line_fragments(raw, &kinds)
        })
        .collect()
}

/// Prettified circuit boxes with value highlighting (port of
/// `ui.rs::build_prettified_highlighted_lines`): one framed box per circuit,
/// blank line between.
fn prettified_highlighted_lines(patch: &Patch, app: &App) -> Vec<LineSpec> {
    let circuits = patch.viewer_circuits();
    let selected = app.selected_component.as_deref();
    let mods: &[ModifierAffect] = match selected {
        Some(tok) => patch.modifier_entries_for(tok),
        None => &[],
    };
    let circuit_store = app.current_circuit_store();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut lines = Vec::new();
    for circuit in &circuits {
        let idx = *counts.get(&circuit.name).unwrap_or(&0);
        let node_id = NodeId::circuit(&circuit.name, idx);
        let display_name = patch.circuit_display_label(&node_id, &circuit_store);
        counts.insert(circuit.name.clone(), idx + 1);
        let color = circuit_color(&circuit.name);
        let entries: Vec<EntrySpec> = circuit
            .entries
            .iter()
            .map(|(key, value)| EntrySpec {
                key: key.clone(),
                value: value.clone(),
                fragments: entry_value_fragments(value, selected, mods),
            })
            .collect();
        lines.extend(prettified_box_lines(&display_name, color, &entries));
        lines.push(LineSpec {
            text: String::new(),
            fragments: vec![],
        });
    }
    lines
}

/// The circuits sidebar (port of `ui.rs::render_source_sidebar` +
/// `sidebar_selected_index`): disambiguated display labels and the entry the
/// cursor or scroll currently sits in.
fn sidebar_spec(patch: &Patch, app: &App) -> SidebarSpec {
    let circuit_store = app.current_circuit_store();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let names: Vec<String> = patch
        .sections
        .iter()
        .map(|s| {
            let idx = *counts.get(&s.name).unwrap_or(&0);
            let node_id = NodeId::circuit(&s.name, idx);
            counts.insert(s.name.clone(), idx + 1);
            patch.circuit_display_label(&node_id, &circuit_store)
        })
        .collect();
    SidebarSpec {
        names: disambiguate_names(&names),
        selected: sidebar_selected_index(patch, app),
    }
}

/// The sidebar entry containing the current position: the section holding the
/// cursor's selected occurrence, else the one holding the scroll line.
fn sidebar_selected_index(patch: &Patch, app: &App) -> Option<usize> {
    if patch.sections.is_empty() {
        return None;
    }
    let target_line = if let Some(tok) = app.selected_component.as_ref() {
        patch
            .occurrences_for(tok)
            .get(app.occurrence_cursor)
            .map(|s| s.line)
    } else {
        None
    }
    .unwrap_or(app.source_scroll);
    let mut idx: Option<usize> = None;
    for (i, sec) in patch.sections.iter().enumerate() {
        if sec.header_span.line <= target_line {
            idx = Some(i);
        } else {
            break;
        }
    }
    idx.or(Some(0))
}

/// Disambiguate repeated circuit labels with a ` (n)` suffix (port of
/// `ui.rs::disambiguate_names`).
fn disambiguate_names(names: &[String]) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut result = Vec::new();
    for name in names {
        let count = counts.entry(name.clone()).or_insert(0);
        if *count == 0 {
            result.push(name.clone());
        } else {
            result.push(format!("{} ({})", name, count));
        }
        *count += 1;
    }
    result
}

/// The minimap column (port of `ui.rs::render_minimap`'s data side): marker
/// rows for occurrence and modifier lines with the viewport band.
fn minimap_spec(patch: &Patch, app: &App) -> MinimapSpec {
    let total_lines = if !patch.raw_lines.is_empty() {
        patch.raw_lines.len()
    } else {
        patch.sections.len().max(1)
    };
    let mut occ = HashSet::new();
    let mut exact = HashSet::new();
    let mut boolean = HashSet::new();
    if let Some(tok) = app.selected_component.as_deref() {
        occ = patch.occurrences_for(tok).iter().map(|s| s.line).collect();
        for affect in patch.modifier_entries_for(tok) {
            if affect.selectat.is_some() {
                exact.insert(affect.span.line);
            } else {
                boolean.insert(affect.span.line);
            }
        }
    }
    MinimapSpec {
        rows: minimap_rows(
            total_lines,
            MINIMAP_ROWS,
            &occ,
            &exact,
            &boolean,
            app.source_scroll,
        ),
    }
}

/// The status-bar fragments (port of `ui.rs::render_viewer_status`): the bold
/// "Source Viewer" title, the hint keys, and the trailing transient message.
/// The paint maps bold fragments to the text token and plain ones to the
/// viewer-key token, so keys stay un-bolded and the title/message bold.
fn viewer_status(app: &App) -> Vec<StatusFragment> {
    let t = crate::theme::active();
    let mut fragments = vec![
        StatusFragment {
            text: "Source Viewer".into(),
            color: t.text,
            bold: true,
        },
        StatusFragment {
            text: " | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "ESC".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " close | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "j/k".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " scroll | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "Up/Down".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " occur | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "Home/End".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " jump | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "t".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " toggle | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "Tab".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " focus | ".into(),
            color: t.text,
            bold: false,
        },
        StatusFragment {
            text: "[ / ]".into(),
            color: t.viewer_key,
            bold: false,
        },
        StatusFragment {
            text: " split".into(),
            color: t.text,
            bold: false,
        },
    ];
    if !app.status_message.is_empty() {
        fragments.push(StatusFragment {
            text: " | ".into(),
            color: t.text,
            bold: false,
        });
        fragments.push(StatusFragment {
            text: app.status_message.clone(),
            color: t.text,
            bold: true,
        });
    }
    fragments
}

fn rgb(color: Color) -> egui::Color32 {
    crate::theme::active().egui_color(color)
}

/// The color of a circuit's box: a plugin-declared `color` token wins over
/// the name-convention match (port of `ui.rs::circuit_color`).
pub(super) fn circuit_color(name: &str) -> Color {
    if let Some(token) = crate::schema::load_schema()
        .circuits
        .get(&name.to_ascii_lowercase())
        .and_then(|c| c.color.as_deref())
    {
        return kind_token_color(token);
    }
    kind_token_color(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(line: usize, col_start: usize, col_end: usize) -> Span {
        Span {
            line,
            col_start,
            col_end,
        }
    }

    fn affect(
        line: usize,
        col_start: usize,
        col_end: usize,
        selectat: Option<&str>,
    ) -> ModifierAffect {
        ModifierAffect {
            span: span(line, col_start, col_end),
            source: String::new(),
            selectat: selectat.map(String::from),
        }
    }

    fn raw_spec(lines: Vec<LineSpec>) -> ViewerSpec {
        ViewerSpec {
            title: " Source [raw] ".into(),
            focused: true,
            lines,
            scroll: 0,
            empty_message: None,
            sidebar: None,
            minimap: None,
            status: vec![],
        }
    }

    fn painted_labels(spec: &ViewerSpec) -> Vec<String> {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(600.0, 300.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_viewer(ui.painter(), ui.max_rect().size(), ui.ctx(), Some(spec));
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
    fn raw_line_kinds_highlight_occurrence_and_modifier() {
        let raw = "pot = P1.1";
        let occ = vec![span(0, 7, 11)];
        let kinds = raw_line_kinds(raw, &occ, &[], Some(span(0, 7, 11)));
        assert_eq!(kinds[7], HighlightKind::OccCurrent);
        assert_eq!(kinds[0], HighlightKind::None);
        // A modifier overrides a non-current occurrence.
        let kinds = raw_line_kinds(raw, &occ, &[affect(0, 0, 3, None)], None);
        assert_eq!(kinds[0], HighlightKind::ModCyan);
        assert_eq!(kinds[7], HighlightKind::OccOther);
        // ... but not the current occurrence.
        let kinds = raw_line_kinds(
            raw,
            &occ,
            &[affect(0, 7, 11, Some("x"))],
            Some(span(0, 7, 11)),
        );
        assert_eq!(kinds[7], HighlightKind::OccCurrent);
    }

    #[test]
    fn line_fragments_split_runs_with_colors() {
        let raw = "pot = P1.1";
        let kinds = raw_line_kinds(raw, &[span(0, 6, 10)], &[], None);
        let line = line_fragments(raw, &kinds);
        assert_eq!(line.text, raw);
        // Only the styled occurrence run becomes a fragment; the plain prefix
        // renders as default theme text.
        assert_eq!(line.fragments.len(), 1);
        assert_eq!(
            &line.text[line.fragments[0].range.0..line.fragments[0].range.1],
            "P1.1"
        );
        assert!(line.fragments[0].bold);
    }

    #[test]
    fn token_spans_in_value_matches_whole_tokens_only() {
        assert_eq!(token_spans_in_value("P1.1", "P1"), vec![]); // dot suffix blocks
        assert_eq!(token_spans_in_value("p1 P1.2", "P1"), vec![]); // dot suffix blocks too
        assert_eq!(token_spans_in_value("p1 P1", "P1"), vec![(3, 5)]);
        assert_eq!(token_spans_in_value("P1", "P1"), vec![(0, 2)]);
    }

    #[test]
    fn entry_value_fragments_mark_modifier_and_occurrence() {
        let mods = vec![ModifierAffect {
            span: span(0, 0, 4),
            source: "P1.1".into(),
            selectat: Some("x".into()),
        }];
        let frags = entry_value_fragments("P1.1", Some("P1"), &mods);
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0].color, crate::theme::active().modifier_exact);
        assert!(frags[0].underlined);
        // Non-modifier value with a token occurrence.
        let frags = entry_value_fragments("in P1.1", Some("P1.1"), &[]);
        assert!(
            frags.iter().any(|f| f.reversed),
            "occurrence highlighted: {frags:?}"
        );
    }

    #[test]
    fn prettified_box_lines_renders_header_entries_footer() {
        let color = crate::theme::active().accent;
        let entries = vec![EntrySpec {
            key: "output".into(),
            value: "P1.1".into(),
            fragments: entry_value_fragments("P1.1", None, &[]),
        }];
        let lines = prettified_box_lines("seq", color, &entries);
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0].text.starts_with('\u{250C}'),
            "top border: {}",
            lines[0].text
        );
        assert!(
            lines[1].text.contains("output = P1.1"),
            "entry line: {}",
            lines[1].text
        );
        assert!(
            lines[2].text.starts_with('\u{2514}'),
            "footer: {}",
            lines[2].text
        );
    }

    #[test]
    fn circuit_color_maps_kind_tokens() {
        let t = crate::theme::active();
        assert_eq!(kind_token_color("led"), t.led);
        assert_eq!(kind_token_color("pot"), t.knob);
        assert_eq!(kind_token_color("unknown"), t.accent);
    }

    #[test]
    fn paint_raw_mode_draws_lines_and_highlight() {
        let raw = "pot = P1.1";
        let kinds = raw_line_kinds(raw, &[span(0, 6, 10)], &[], None);
        let lines = vec![line_fragments(raw, &kinds)];
        let labels = painted_labels(&raw_spec(lines));
        assert!(
            labels.iter().any(|l| l.contains("Source [raw]")),
            "title: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("pot =")),
            "plain prefix: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("P1.1")),
            "highlighted token: {labels:?}"
        );
    }

    #[test]
    fn paint_empty_message_centered() {
        let labels = painted_labels(&ViewerSpec {
            empty_message: Some("No circuits in patch".into()),
            ..raw_spec(vec![])
        });
        assert!(
            labels.iter().any(|l| l.contains("No circuits in patch")),
            "{labels:?}"
        );
    }

    #[test]
    fn paint_sidebar_and_minimap_columns() {
        let spec = ViewerSpec {
            title: " Source [prettified] ".into(),
            focused: false,
            lines: vec![],
            scroll: 0,
            empty_message: None,
            sidebar: Some(SidebarSpec {
                names: vec!["seq".into(), "copier".into()],
                selected: Some(0),
            }),
            minimap: Some(MinimapSpec {
                rows: minimap_rows(
                    10,
                    8,
                    &HashSet::from([1]),
                    &HashSet::new(),
                    &HashSet::from([5]),
                    2,
                ),
            }),
            status: vec![],
        };
        let labels = painted_labels(&spec);
        assert!(
            labels.iter().any(|l| l.contains("Circuits")),
            "sidebar title: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("seq")),
            "sidebar entry: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("Map")),
            "minimap title: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains('\u{2588}')),
            "occurrence marker: {labels:?}"
        );
    }

    #[test]
    fn minimap_rows_mark_occurrences_and_viewport() {
        let rows = minimap_rows(
            10,
            10,
            &HashSet::from([1]),
            &HashSet::new(),
            &HashSet::from([5]),
            2,
        );
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[1].ch, '\u{2588}', "occurrence row");
        assert_eq!(rows[5].ch, '\u{2593}', "modifier row");
        assert!(!rows[0].reversed, "viewport starts at scroll 2");
        assert!(rows[2].reversed, "viewport covers scroll 2");
    }

    #[test]
    fn pane_split_hides_minimap_then_sidebar_when_narrow() {
        let (s, c, m) = pane_split(300.0, true, true, 80.0).unwrap();
        assert!(s > 0.0 && m == 26.0 && c >= 80.0, "wide: {s} {c} {m}");
        let (s, c, m) = pane_split(90.0, true, true, 80.0).unwrap();
        assert_eq!(m, 0.0, "minimap hidden first: {s} {c} {m}");
        assert_eq!(
            s, 0.0,
            "sidebar hides too when content still too narrow: {s} {c} {m}"
        );
        assert!(c >= 80.0);
        assert!(pane_split(60.0, true, true, 80.0).is_none(), "nothing fits");
    }

    #[test]
    fn viewer_frame_reports_scroll_over_content_only() {
        let content = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0));
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(600.0, 300.0),
            )),
            events: vec![
                egui::Event::PointerMoved(Pos2::new(200.0, 100.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -40.0),
                    modifiers: egui::Modifiers::default(),
                    phase: egui::TouchPhase::Move,
                },
            ],
            ..Default::default()
        };
        let mut frame = ViewerFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = ui.ctx().input(|i| viewer_frame(i, content));
        });
        full_output.textures_delta.clear();
        assert!(
            frame.scroll_delta < 0.0,
            "wheel scroll reported: {}",
            frame.scroll_delta
        );

        // Pointer outside the content column: no scroll report.
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(600.0, 300.0),
            )),
            events: vec![
                egui::Event::PointerMoved(Pos2::new(500.0, 250.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -40.0),
                    modifiers: egui::Modifiers::default(),
                    phase: egui::TouchPhase::Move,
                },
            ],
            ..Default::default()
        };
        let mut frame = ViewerFrame::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            frame = ui.ctx().input(|i| viewer_frame(i, content));
        });
        full_output.textures_delta.clear();
        assert_eq!(frame.scroll_delta, 0.0);
    }

    #[test]
    fn viewer_spec_no_patch_shows_empty_message() {
        let app = App::new();
        let spec = viewer_spec(&app);
        assert_eq!(spec.title, " Source [raw] ");
        assert!(!spec.focused);
        assert!(spec.lines.is_empty());
        assert_eq!(spec.empty_message.as_deref(), Some("No patch loaded"));
        assert!(spec.sidebar.is_none());
        assert!(spec.minimap.is_none());
        assert!(spec.status.iter().any(|f| f.text == "Source Viewer"));
    }

    #[test]
    fn viewer_spec_raw_builds_lines_sidebar_and_minimap() {
        let content = "[seq]\noutput = P1.1\ninput = P2.2\n";
        let patch = Patch::from_ini_str(content, "test".to_string()).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.selected_component = Some("P1.1".to_string());
        app.occurrence_cursor = 0;
        let spec = viewer_spec(&app);
        assert_eq!(spec.lines.len(), 3);
        assert!(
            spec.lines[1].fragments.iter().any(|f| f.range == (9, 13)),
            "P1.1 highlighted: {:?}",
            spec.lines[1]
        );
        assert!(spec.lines[1].fragments.iter().any(|f| f.reversed));
        let sidebar = spec.sidebar.as_ref().unwrap();
        assert_eq!(sidebar.names, vec!["seq".to_string()]);
        assert_eq!(sidebar.selected, Some(0));
        assert!(spec.minimap.is_some());
        assert_eq!(spec.empty_message, None);
    }

    #[test]
    fn viewer_spec_prettified_builds_boxes() {
        let content = "[seq]\noutput = P1.1\n";
        let patch = Patch::from_ini_str(content, "test".to_string()).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.source_view_mode = SourceViewMode::Prettified;
        let spec = viewer_spec(&app);
        assert!(spec.lines.len() >= 3);
        assert!(spec.lines[0].text.starts_with('\u{250C}'));
        assert_eq!(spec.empty_message, None);
        assert!(spec.sidebar.is_some());
    }

    #[test]
    fn viewer_spec_prettified_empty_circuits_message() {
        // The parser rejects section-less patches, so empty the sections of a
        // valid one: the raw lines stay, mirroring a preamble-only patch.
        let content = "[seq]\noutput = P1.1\n";
        let mut patch = Patch::from_ini_str(content, "test".to_string()).unwrap();
        patch.sections = vec![];
        let mut app = App::new();
        app.patch = Some(patch);
        app.source_view_mode = SourceViewMode::Prettified;
        let spec = viewer_spec(&app);
        assert_eq!(spec.empty_message.as_deref(), Some("No circuits in patch"));
        assert!(spec.lines.is_empty());
        assert!(spec.sidebar.is_none());
    }

    #[test]
    fn viewer_spec_focus_follows_tile_stack() {
        let mut app = App::new();
        app.tile_stack.slots = vec![ViewType::SourceViewer];
        app.tile_stack.focus = FocusSlot::Slot(0);
        assert!(viewer_spec(&app).focused);
        app.tile_stack.focus = FocusSlot::Panels;
        assert!(!viewer_spec(&app).focused);
    }
}
