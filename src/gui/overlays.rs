//! Overlays surface (task 2.5): validation modal, select-state menu,
//! label editor, diff surface, and latency optimizer, ported from
//! `ui.rs::render_validation_modal` / `render_select_menu` / `render_overlay`
//! / `render_optimizer_pane` (and the diff report rendering) to egui draw
//! routines.
//!
//! Each overlay follows the other surface pattern: a fully-resolved pure-data
//! payload built each frame, a draw routine that only paints it, and pure
//! helpers that keep the status/row logic testable headless.

use egui::{Context, Painter, Pos2, Rect, Vec2};
use ratatui::style::Color;

use crate::diff::DiffReport;
use crate::validation::{Severity, ValidationIssue};

// ── Validation modal ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ValidationRow {
    pub location: String,
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ValidationSpec {
    pub title: String,
    pub hint: String,
    pub rows: Vec<ValidationRow>,
    pub empty_message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ValidationFrame {
    pub hovered: Option<usize>,
}

pub(super) fn validation_spec(issues: &[ValidationIssue], cursor: usize) -> ValidationSpec {
    let count = issues.len();
    let err = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let warn = issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let hint = count.saturating_sub(err + warn);
    let title = if count == 0 {
        " Validation (0) ".to_string()
    } else {
        format!(" Validation ({count}) {err}E {warn}W {hint}H ")
    };
    let hint_str = " e:toggle j/k:navigate Enter:jump Esc:close ".to_string();
    if count == 0 {
        return ValidationSpec {
            title,
            hint: hint_str,
            rows: vec![],
            empty_message: Some("No validation issues".to_string()),
        };
    }
    let cursor = cursor.min(count.saturating_sub(1));
    let mut rows = Vec::with_capacity(count);
    for (idx, issue) in issues.iter().enumerate() {
        let loc = format!("L{}:{}", issue.span.line + 1, issue.span.col_start + 1);
        rows.push(ValidationRow {
            location: loc,
            severity: issue.severity,
            code: issue.code.clone(),
            message: issue.message.clone(),
            selected: idx == cursor,
        });
    }
    ValidationSpec {
        title,
        hint: hint_str,
        rows,
        empty_message: None,
    }
}

pub(super) fn paint_validation_modal(
    painter: &Painter,
    canvas: Vec2,
    _ctx: &Context,
    spec: Option<&ValidationSpec>,
) -> ValidationFrame {
    let Some(spec) = spec else {
        return ValidationFrame::default();
    };
    let t = crate::theme::active();
    let w = (canvas.x * 0.60)
        .clamp(40.0, 80.0 * 10.0)
        .min(canvas.x - 8.0)
        .max(24.0 * 8.0);
    let h = (canvas.y * 0.70)
        .clamp(12.0 * 16.0, 40.0 * 16.0)
        .min(canvas.y - 8.0);
    // Fallback to canvas-relative sizing similar to ui.rs centered logic.
    let cw = (canvas.x * 0.60).clamp(24.0 * 6.0, 80.0 * 6.0);
    let ch = (canvas.y * 0.70).clamp(10.0 * 12.0, 40.0 * 12.0);
    let rect = Rect::from_center_size(
        Pos2::new(canvas.x / 2.0, canvas.y / 2.0),
        Vec2::new(cw.min(canvas.x - 8.0), ch.min(canvas.y - 8.0)),
    );
    let _ = (w, h);
    painter.rect_filled(rect, 8.0, rgb(t.muted).gamma_multiply(0.12));
    painter.rect(
        rect,
        8.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.5, rgb(t.validation_modal_border)),
        egui::StrokeKind::Inside,
    );
    // Title
    painter.text(
        rect.min + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        &spec.title,
        egui::FontId::proportional(12.0),
        rgb(t.text),
    );
    painter.text(
        rect.min + egui::vec2(8.0, rect.height() - 14.0),
        egui::Align2::LEFT_TOP,
        &spec.hint,
        egui::FontId::proportional(10.0),
        rgb(t.muted),
    );
    let inner_top = rect.min.y + 24.0;
    let inner_bottom = rect.max.y - 18.0;
    if let Some(msg) = &spec.empty_message {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(12.0),
            rgb(t.muted),
        );
        return ValidationFrame::default();
    }
    let row_h = 16.0;
    let mut y = inner_top;
    let sev_color = |s: Severity| match s {
        Severity::Error => t.validation_error,
        Severity::Warning => t.validation_warning,
        Severity::Hint => t.validation_hint,
    };
    for row in &spec.rows {
        if y + row_h > inner_bottom {
            break;
        }
        if row.selected {
            let r = Rect::from_min_size(
                Pos2::new(rect.min.x + 4.0, y),
                Vec2::new(rect.width() - 8.0, row_h),
            );
            painter.rect_filled(r, 2.0, rgb(t.validation_selected_bg));
        }
        let sev_label = match row.severity {
            Severity::Error => "E",
            Severity::Warning => "W",
            Severity::Hint => "H",
        };
        let text = format!(
            "{} [{}] [{}] {}",
            row.location, sev_label, row.code, row.message
        );
        let fg = if row.selected {
            rgb(t.text)
        } else {
            rgb(sev_color(row.severity)).gamma_multiply(0.85)
        };
        painter.text(
            Pos2::new(rect.min.x + 8.0, y + 1.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::monospace(11.0),
            fg,
        );
        y += row_h;
    }
    ValidationFrame::default()
}

// ── Select-state menu ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SelectRow {
    pub signal: String,
    pub kind_label: String,
    pub usage: usize,
    pub candidates: String,
    pub current: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SelectMenuSpec {
    pub title: String,
    pub hint: String,
    pub rows: Vec<SelectRow>,
    pub empty_message: Option<String>,
}

pub(super) fn paint_select_menu(
    painter: &Painter,
    canvas: Vec2,
    _ctx: &Context,
    spec: Option<&SelectMenuSpec>,
) {
    let Some(spec) = spec else {
        return;
    };
    let t = crate::theme::active();
    let cw = (canvas.x * 0.60).clamp(24.0 * 6.0, 80.0 * 6.0);
    let ch = (canvas.y * 0.70).clamp(10.0 * 12.0, 40.0 * 12.0);
    let rect = Rect::from_center_size(
        Pos2::new(canvas.x / 2.0, canvas.y / 2.0),
        Vec2::new(cw.min(canvas.x - 8.0), ch.min(canvas.y - 8.0)),
    );
    painter.rect_filled(rect, 8.0, rgb(t.muted).gamma_multiply(0.12));
    painter.rect(
        rect,
        8.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.5, rgb(t.validation_modal_border)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.min + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        &spec.title,
        egui::FontId::proportional(12.0),
        rgb(t.text),
    );
    painter.text(
        rect.min + egui::vec2(8.0, rect.height() - 14.0),
        egui::Align2::LEFT_TOP,
        &spec.hint,
        egui::FontId::proportional(10.0),
        rgb(t.muted),
    );
    if let Some(msg) = &spec.empty_message {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(12.0),
            rgb(t.muted),
        );
        return;
    }
    let mut y = rect.min.y + 24.0;
    let bottom = rect.max.y - 18.0;
    for row in &spec.rows {
        if y + 16.0 > bottom {
            break;
        }
        if row.selected {
            let r = Rect::from_min_size(
                Pos2::new(rect.min.x + 4.0, y),
                Vec2::new(rect.width() - 8.0, 16.0),
            );
            painter.rect_filled(r, 2.0, rgb(t.validation_selected_bg));
        }
        let text = format!(
            "{} [{}] x{}  candidates: {}  current: {}",
            row.signal, row.kind_label, row.usage, row.candidates, row.current
        );
        painter.text(
            Pos2::new(rect.min.x + 8.0, y + 1.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::monospace(10.0),
            rgb(t.text),
        );
        y += 16.0;
    }
}

// ── Label editor ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LabelEditSpec {
    pub draft: String,
    pub hint: String,
    pub hue: Option<Color>,
}

pub(super) fn paint_label_editor(
    painter: &Painter,
    canvas: Vec2,
    _ctx: &Context,
    spec: Option<&LabelEditSpec>,
) {
    let Some(spec) = spec else {
        return;
    };
    let t = crate::theme::active();
    let hue = spec.hue.map(|c| rgb(c)).unwrap_or_else(|| rgb(t.text));
    let cw = (canvas.x * 0.60).clamp(40.0 * 6.0, 70.0 * 6.0);
    let rect = Rect::from_center_size(
        Pos2::new(canvas.x / 2.0, canvas.y / 2.0),
        Vec2::new(cw.min(canvas.x - 8.0), 80.0),
    );
    painter.rect_filled(rect, 8.0, rgb(t.muted).gamma_multiply(0.18));
    painter.rect(
        rect,
        8.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.5, hue),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.min + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        " Edit Label ",
        egui::FontId::proportional(12.0),
        hue,
    );
    let input = format!("{}▌", spec.draft);
    painter.text(
        rect.min + egui::vec2(8.0, 28.0),
        egui::Align2::LEFT_TOP,
        input,
        egui::FontId::monospace(13.0),
        rgb(t.text),
    );
    painter.text(
        rect.min + egui::vec2(8.0, 52.0),
        egui::Align2::LEFT_TOP,
        &spec.hint,
        egui::FontId::proportional(10.0),
        hue,
    );
}

// ── Diff surface ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DiffSpec {
    pub title: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

pub(super) fn paint_diff_surface(
    painter: &Painter,
    canvas: Vec2,
    _ctx: &Context,
    spec: Option<&DiffSpec>,
) {
    let Some(spec) = spec else {
        return;
    };
    let t = crate::theme::active();
    let rect = Rect::from_min_size(Pos2::ZERO, canvas);
    painter.rect_filled(rect, 0.0, rgb(t.muted).gamma_multiply(0.06));
    painter.text(
        rect.min + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        &spec.title,
        egui::FontId::proportional(12.0),
        rgb(t.text),
    );
    let mut y = rect.min.y + 26.0;
    let draw_section =
        |painter: &Painter, y: &mut f32, label: &str, items: &[String], color: Color| {
            if items.is_empty() {
                return;
            }
            painter.text(
                Pos2::new(rect.min.x + 8.0, *y),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(11.0),
                rgb(color),
            );
            *y += 16.0;
            for item in items.iter().take(12) {
                if *y + 14.0 > rect.max.y - 8.0 {
                    break;
                }
                painter.text(
                    Pos2::new(rect.min.x + 16.0, *y),
                    egui::Align2::LEFT_TOP,
                    item,
                    egui::FontId::monospace(10.0),
                    rgb(t.text),
                );
                *y += 14.0;
            }
            *y += 6.0;
        };
    draw_section(
        painter,
        &mut y,
        "Added",
        &spec.added,
        t.graph_edge_diff_added,
    );
    draw_section(
        painter,
        &mut y,
        "Removed",
        &spec.removed,
        t.graph_edge_diff_removed,
    );
    draw_section(
        painter,
        &mut y,
        "Changed",
        &spec.changed,
        t.graph_edge_diff_added,
    );
    if spec.added.is_empty() && spec.removed.is_empty() && spec.changed.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No differences",
            egui::FontId::proportional(12.0),
            rgb(t.muted),
        );
    }
}

pub(super) fn diff_spec_from_report(report: Option<&DiffReport>) -> DiffSpec {
    let Some(report) = report else {
        return DiffSpec {
            title: " Diff (0) ".to_string(),
            added: vec![],
            removed: vec![],
            changed: vec![],
        };
    };
    let added = report.added_cables.clone();
    let removed = report.removed_cables.clone();
    let changed: Vec<String> = report
        .changed_cables
        .iter()
        .map(|c| c.cable.clone())
        .collect();
    let total = added.len() + removed.len() + changed.len();
    DiffSpec {
        title: format!(" Diff ({total}) "),
        added,
        removed,
        changed,
    }
}

// ── Latency optimizer ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OptimizerRow {
    pub label: String,
    pub weighted_obj: f32,
    pub avg_before: f32,
    pub avg_after: f32,
    pub max_before: f32,
    pub max_after: f32,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OptimizerSpec {
    pub header: String,
    pub hint: String,
    pub rows: Vec<OptimizerRow>,
    pub empty_message: Option<String>,
}

pub(super) fn paint_optimizer(
    painter: &Painter,
    canvas: Vec2,
    _ctx: &Context,
    spec: Option<&OptimizerSpec>,
) {
    let Some(spec) = spec else {
        return;
    };
    let t = crate::theme::active();
    let rect = Rect::from_min_size(Pos2::ZERO, canvas);
    painter.rect_filled(rect, 0.0, rgb(t.muted).gamma_multiply(0.04));
    painter.text(
        rect.min + egui::vec2(8.0, 6.0),
        egui::Align2::LEFT_TOP,
        &spec.header,
        egui::FontId::proportional(12.0),
        rgb(t.text),
    );
    if let Some(msg) = &spec.empty_message {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(12.0),
            rgb(t.muted),
        );
        return;
    }
    let mut y = rect.min.y + 24.0;
    for row in &spec.rows {
        if y + 16.0 > rect.max.y - 18.0 {
            break;
        }
        if row.selected {
            let r = Rect::from_min_size(
                Pos2::new(rect.min.x + 2.0, y),
                Vec2::new(rect.width() - 4.0, 16.0),
            );
            painter.rect_filled(r, 2.0, rgb(t.optimizer_selected_bg));
        }
        let text = format!(
            "{} obj {:.2} · avg {:.2}→{:.2} · max {:.2}→{:.2}",
            row.label,
            row.weighted_obj,
            row.avg_before,
            row.avg_after,
            row.max_before,
            row.max_after
        );
        let color = if row.selected { t.text } else { t.muted };
        painter.text(
            Pos2::new(rect.min.x + 8.0, y + 1.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::monospace(10.0),
            rgb(color),
        );
        y += 16.0;
    }
    painter.text(
        Pos2::new(rect.min.x + 8.0, rect.max.y - 14.0),
        egui::Align2::LEFT_TOP,
        &spec.hint,
        egui::FontId::proportional(10.0),
        rgb(t.muted),
    );
}

fn rgb(color: Color) -> egui::Color32 {
    crate::theme::active().egui_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Vec2};

    fn painted_labels<F: FnMut(&egui::Painter, Vec2, &egui::Context)>(
        size: Vec2,
        mut f: F,
    ) -> Vec<String> {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            f(ui.painter(), ui.max_rect().size(), ui.ctx());
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
    fn validation_modal_renders_title_and_issue() {
        let issues = vec![ValidationIssue {
            span: crate::patch::Span {
                line: 0,
                col_start: 0,
                col_end: 4,
            },
            severity: Severity::Error,
            code: "unknown_circuit".into(),
            message: "unknown circuit foo".into(),
        }];
        let spec = validation_spec(&issues, 0);
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_validation_modal(p, c, ctx, Some(&spec));
        });
        assert!(
            labels.iter().any(|l| l.contains("Validation")),
            "{labels:?}"
        );
        assert!(labels.iter().any(|l| l.contains("L1:1")), "{labels:?}");
        assert!(
            labels.iter().any(|l| l.contains("unknown_circuit")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("unknown circuit foo")),
            "{labels:?}"
        );
    }

    #[test]
    fn validation_modal_empty_state() {
        let spec = validation_spec(&[], 0);
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_validation_modal(p, c, ctx, Some(&spec));
        });
        assert!(
            labels.iter().any(|l| l.contains("No validation issues")),
            "{labels:?}"
        );
    }

    #[test]
    fn select_menu_renders_signal_row() {
        let spec = SelectMenuSpec {
            title: " Select state (1) ".into(),
            hint: " j/k:navigate [/]:cycle Esc:clear ".into(),
            rows: vec![SelectRow {
                signal: "sel".into(),
                kind_label: "register".into(),
                usage: 2,
                candidates: "0, 1".into(),
                current: "1".into(),
                selected: true,
            }],
            empty_message: None,
        };
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_select_menu(p, c, ctx, Some(&spec));
        });
        assert!(
            labels.iter().any(|l| l.contains("Select state")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("sel [register]")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("candidates: 0, 1")),
            "{labels:?}"
        );
    }

    #[test]
    fn select_menu_empty_state() {
        let spec = SelectMenuSpec {
            title: " Select state (0) ".into(),
            hint: " j/k:navigate [/]:cycle Esc:clear ".into(),
            rows: vec![],
            empty_message: Some("No select signals".into()),
        };
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_select_menu(p, c, ctx, Some(&spec));
        });
        assert!(
            labels.iter().any(|l| l.contains("No select signals")),
            "{labels:?}"
        );
    }

    #[test]
    fn label_editor_renders_draft_and_hint() {
        let spec = LabelEditSpec {
            draft: "MyLabel".into(),
            hint: "Enter save | Esc cancel | 1..4 layer".into(),
            hue: Some(crate::theme::active().shift1),
        };
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_label_editor(p, c, ctx, Some(&spec));
        });
        assert!(
            labels.iter().any(|l| l.contains("Edit Label")),
            "{labels:?}"
        );
        assert!(labels.iter().any(|l| l.contains("MyLabel")), "{labels:?}");
        assert!(
            labels.iter().any(|l| l.contains("Enter save")),
            "{labels:?}"
        );
    }

    #[test]
    fn diff_surface_renders_added_removed_changed() {
        let spec = DiffSpec {
            title: " Diff (3) ".into(),
            added: vec!["_CABLE_A".into()],
            removed: vec!["_CABLE_B".into()],
            changed: vec!["_CABLE_C".into()],
        };
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_diff_surface(p, c, ctx, Some(&spec));
        });
        assert!(labels.iter().any(|l| l.contains("Diff (3)")), "{labels:?}");
        assert!(labels.iter().any(|l| l.contains("_CABLE_A")), "{labels:?}");
        assert!(labels.iter().any(|l| l.contains("_CABLE_B")), "{labels:?}");
        assert!(labels.iter().any(|l| l.contains("_CABLE_C")), "{labels:?}");
    }

    #[test]
    fn optimizer_renders_weighted_row() {
        let spec = OptimizerSpec {
            header: " Optimizer (1) · w = 0.5 ".into(),
            hint: " j/k select · Enter preview · r restore · s export · Esc close ".into(),
            rows: vec![OptimizerRow {
                label: "candidate 1".into(),
                weighted_obj: 1.23,
                avg_before: 2.0,
                avg_after: 1.5,
                max_before: 5.0,
                max_after: 3.0,
                selected: true,
            }],
            empty_message: None,
        };
        let labels = painted_labels(Vec2::new(400.0, 300.0), |p, c, ctx| {
            paint_optimizer(p, c, ctx, Some(&spec));
        });
        assert!(labels.iter().any(|l| l.contains("Optimizer")), "{labels:?}");
        assert!(
            labels.iter().any(|l| l.contains("candidate 1")),
            "{labels:?}"
        );
        assert!(labels.iter().any(|l| l.contains("obj 1.23")), "{labels:?}");
    }

    #[test]
    fn overlays_none_draws_nothing() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let mut full_output = ctx.run_ui(raw_input, |ui| {
            paint_validation_modal(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
            paint_select_menu(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
            paint_label_editor(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
            paint_diff_surface(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
            paint_optimizer(ui.painter(), ui.max_rect().size(), ui.ctx(), None);
        });
        full_output.textures_delta.clear();
        assert!(full_output.shapes.is_empty());
    }
}
