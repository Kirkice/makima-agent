//! Unified diff view component for file changes.
//!
//! Inspired by Zoo-Code's FileChangesPanel: renders file-level diffs
//! with line numbers, color-coded additions/deletions, and collapsible sections.

use eframe::egui::{self, Color32, CornerRadius, RichText};

use crate::theme::colors;

/// A single diff hunk (a contiguous block of changes)
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<DiffLine>,
}

/// A single line in a diff
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

/// A file change entry
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub additions: usize,
    pub deletions: usize,
}

/// Colors for diff rendering
const DIFF_ADD_BG: Color32 = Color32::from_rgb(22, 60, 22);
const DIFF_ADD_FG: Color32 = Color32::from_rgb(120, 220, 120);
const DIFF_DEL_BG: Color32 = Color32::from_rgb(60, 22, 22);
const DIFF_DEL_FG: Color32 = Color32::from_rgb(220, 120, 120);
const DIFF_CTX_FG: Color32 = Color32::from_rgb(180, 180, 180);
const DIFF_LINE_NUM: Color32 = Color32::from_rgb(100, 100, 120);
const DIFF_GUTTER_BG: Color32 = Color32::from_rgb(28, 30, 38);
const DIFF_CODE_BG: Color32 = Color32::from_rgb(22, 24, 32);

/// Draw a list of file changes with unified diff view.
pub fn draw_file_changes(ui: &mut egui::Ui, changes: &[FileChange]) {
    if changes.is_empty() {
        return;
    }

    for change in changes {
        draw_file_change(ui, change);
        ui.add_space(8.0);
    }
}

/// Draw a single file change with collapsible diff view.
fn draw_file_change(ui: &mut egui::Ui, change: &FileChange) {
    let header_text = format!(
        "📝 {}  +{} -{}",
        change.path, change.additions, change.deletions
    );

    egui::CollapsingHeader::new(
        RichText::new(&header_text)
            .size(12.0)
            .color(colors::TEXT_PRIMARY)
            .strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        for hunk in &change.hunks {
            draw_diff_hunk(ui, hunk);
            ui.add_space(4.0);
        }
    });
}

/// Draw a single diff hunk with line numbers and color-coded lines.
fn draw_diff_hunk(ui: &mut egui::Ui, hunk: &DiffHunk) {
    egui::Frame::NONE
        .fill(DIFF_CODE_BG)
        .stroke(egui::Stroke::new(1.0, colors::BORDER_WEAK))
        .corner_radius(CornerRadius::same(6))
        .show(ui, |ui| {
            egui::Grid::new(format!("diff_hunk_{}", hunk.old_start))
                .striped(false)
                .spacing(egui::vec2(0.0, 0.0))
                .min_col_width(0.0)
                .show(ui, |ui| {
                    for line in &hunk.lines {
                        draw_diff_line(ui, line);
                    }
                });
        });
}

/// Draw a single diff line with line numbers and coloring.
fn draw_diff_line(ui: &mut egui::Ui, line: &DiffLine) {
    let bg = match line.kind {
        DiffLineKind::Addition => DIFF_ADD_BG,
        DiffLineKind::Deletion => DIFF_DEL_BG,
        DiffLineKind::Context => Color32::TRANSPARENT,
    };

    let fg = match line.kind {
        DiffLineKind::Addition => DIFF_ADD_FG,
        DiffLineKind::Deletion => DIFF_DEL_FG,
        DiffLineKind::Context => DIFF_CTX_FG,
    };

    let prefix = match line.kind {
        DiffLineKind::Addition => "+",
        DiffLineKind::Deletion => "-",
        DiffLineKind::Context => " ",
    };

    let old_num = line
        .old_lineno
        .map(|n| format!("{:>4}", n))
        .unwrap_or_else(|| "    ".to_string());

    ui.allocate_ui_with_layout(
        egui::vec2(40.0, 0.0),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            egui::Frame::NONE.fill(DIFF_GUTTER_BG).show(ui, |ui| {
                ui.label(
                    RichText::new(&old_num)
                        .size(11.0)
                        .monospace()
                        .color(DIFF_LINE_NUM),
                );
            });
        },
    );

    let new_num = line
        .new_lineno
        .map(|n| format!("{:>4}", n))
        .unwrap_or_else(|| "    ".to_string());

    ui.allocate_ui_with_layout(
        egui::vec2(40.0, 0.0),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            egui::Frame::NONE.fill(DIFF_GUTTER_BG).show(ui, |ui| {
                ui.label(
                    RichText::new(&new_num)
                        .size(11.0)
                        .monospace()
                        .color(DIFF_LINE_NUM),
                );
            });
        },
    );

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 0.0),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            egui::Frame::NONE
                .fill(bg)
                .inner_margin(egui::Margin::symmetric(8, 2))
                .show(ui, |ui| {
                    let display = format!("{}{}", prefix, line.content);
                    ui.label(
                        RichText::new(&display)
                            .size(12.0)
                            .monospace()
                            .color(fg),
                    );
                });
        },
    );

    ui.end_row();
}

/// Parse unified diff text into FileChange entries.
pub fn parse_unified_diff(diff_text: &str) -> Vec<FileChange> {
    let mut changes = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_hunks: Vec<DiffHunk> = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for line in diff_text.lines() {
        if line.starts_with("diff --git") || line.starts_with("--- ") {
            if let Some(path) = current_file.take() {
                if let Some(hunk) = current_hunk.take() {
                    current_hunks.push(hunk);
                }
                if !current_hunks.is_empty() {
                    changes.push(FileChange {
                        path,
                        hunks: std::mem::take(&mut current_hunks),
                        additions,
                        deletions,
                    });
                    additions = 0;
                    deletions = 0;
                }
            }
            continue;
        }

        if line.starts_with("+++ ") {
            let path = line.strip_prefix("+++ ").unwrap_or("").trim();
            let path = path.strip_prefix("b/").unwrap_or(path);
            current_file = Some(path.to_string());
            continue;
        }

        if line.starts_with("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                current_hunks.push(hunk);
            }
            let (old_start, new_start) = parse_hunk_header(line);
            current_hunk = Some(DiffHunk {
                old_start,
                new_start,
                lines: Vec::new(),
            });
            continue;
        }

        if let Some(ref mut hunk) = current_hunk {
            let old_count = hunk.lines.iter().filter(|l| l.old_lineno.is_some()).count();
            let new_count = hunk.lines.iter().filter(|l| l.new_lineno.is_some()).count();
            let old_line = hunk.old_start + old_count;
            let new_line = hunk.new_start + new_count;

            if line.starts_with('+') {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Addition,
                    content: line[1..].to_string(),
                    old_lineno: None,
                    new_lineno: Some(new_line),
                });
                additions += 1;
            } else if line.starts_with('-') {
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Deletion,
                    content: line[1..].to_string(),
                    old_lineno: Some(old_line),
                    new_lineno: None,
                });
                deletions += 1;
            } else if !line.is_empty() || line.starts_with(' ') {
                let content = if line.starts_with(' ') {
                    line[1..].to_string()
                } else {
                    line.to_string()
                };
                hunk.lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content,
                    old_lineno: Some(old_line),
                    new_lineno: Some(new_line),
                });
            }
        }
    }

    // Flush last file
    if let Some(path) = current_file.take() {
        if let Some(hunk) = current_hunk.take() {
            current_hunks.push(hunk);
        }
        if !current_hunks.is_empty() {
            changes.push(FileChange {
                path,
                hunks: current_hunks,
                additions,
                deletions,
            });
        }
    }

    changes
}

fn parse_hunk_header(line: &str) -> (usize, usize) {
    let content = line
        .strip_prefix("@@ ")
        .and_then(|s| s.split(" @@").next())
        .unwrap_or("");

    let parts: Vec<&str> = content.split_whitespace().collect();
    let old_start = parts
        .first()
        .and_then(|s| s.strip_prefix('-'))
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let new_start = parts
        .get(1)
        .and_then(|s| s.strip_prefix('+'))
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    (old_start, new_start)
}