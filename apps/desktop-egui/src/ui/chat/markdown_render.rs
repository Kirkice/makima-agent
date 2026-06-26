//! Lightweight Markdown renderer for chat bubbles.
//!
//! Uses pulldown-cmark to parse markdown and renders to egui widgets.
//! Supports: bold, italic, code spans, fenced code blocks, links, lists, headings.

use eframe::egui::{self, Color32, CornerRadius, RichText};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::theme::colors;

/// Render markdown text into the egui UI.
pub fn draw_markdown(ui: &mut egui::Ui, text: &str) {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(text, opts);

    // Collect all events first, then render
    let events: Vec<Event> = parser.collect();
    let mut ctx = RenderCtx::new();

    for event in events {
        ctx.process(event);
    }
    ctx.flush(ui);
}

// ── Render context ───────────────────────────────────────────────────

struct RenderCtx {
    blocks: Vec<Block>,
    // Current paragraph fragments
    frags: Vec<RichText>,
    // Inline style
    bold: bool,
    italic: bool,
    strike: bool,
    // Code block
    in_code: bool,
    code_lang: String,
    code_buf: String,
    // Heading
    heading: u8,
    // List
    list_kind: Vec<ListKind>,
    list_idx: Vec<u32>,
}

enum Block {
    Paragraph(Vec<RichText>),
    CodeBlock(String, String), // (lang, code)
    Heading(u8, Vec<RichText>),
    Rule,
    ListItem(ListKind, u32, Vec<RichText>),
}

#[derive(Clone, Copy, PartialEq)]
enum ListKind {
    Bullet,
    Numbered,
}

impl RenderCtx {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            frags: Vec::new(),
            bold: false,
            italic: false,
            strike: false,
            in_code: false,
            code_lang: String::new(),
            code_buf: String::new(),
            heading: 0,
            list_kind: Vec::new(),
            list_idx: Vec::new(),
        }
    }

    fn styled(&self, s: &str) -> RichText {
        let mut rt = RichText::new(s).size(13.5);
        if self.bold { rt = rt.strong(); }
        if self.italic { rt = rt.italics(); }
        if self.strike { rt = rt.strikethrough(); }
        if self.heading > 0 {
            let sz = match self.heading {
                1 => 18.0, 2 => 16.0, 3 => 15.0, _ => 14.0,
            };
            rt = rt.size(sz).strong();
        }
        rt
    }

    fn process(&mut self, ev: Event) {
        match ev {
            Event::Text(t) => {
                if self.in_code {
                    self.code_buf.push_str(&t);
                } else {
                    self.frags.push(self.styled(&t));
                }
            }
            Event::Code(c) => {
                self.frags.push(
                    RichText::new(c.to_string())
                        .code()
                        .size(12.5)
                        .background_color(Color32::from_rgb(35, 38, 48)),
                );
            }
            Event::SoftBreak | Event::HardBreak => {
                if !self.in_code {
                    self.frags.push(RichText::new("\n").size(13.5));
                }
            }
            Event::Start(tag) => match tag {
                Tag::Paragraph => { self.frags.clear(); }
                Tag::Heading { level, .. } => {
                    self.heading = level as u8;
                    self.frags.clear();
                }
                Tag::Strong => self.bold = true,
                Tag::Emphasis => self.italic = true,
                Tag::Strikethrough => self.strike = true,
                Tag::CodeBlock(kind) => {
                    self.in_code = true;
                    self.code_buf.clear();
                    self.code_lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(l) => l.to_string(),
                        _ => String::new(),
                    };
                }
                Tag::List(start) => {
                    let k = if start.is_some() { ListKind::Numbered } else { ListKind::Bullet };
                    self.list_kind.push(k);
                    self.list_idx.push(0);
                }
                Tag::Item => {
                    if let Some(n) = self.list_idx.last_mut() { *n += 1; }
                    self.frags.clear();
                }
                Tag::Link { dest_url, .. } => {
                    self.frags.push(RichText::new(dest_url.to_string())
                        .size(13.5).color(colors::INFO).underline());
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => {
                    let f = std::mem::take(&mut self.frags);
                    if !f.is_empty() { self.blocks.push(Block::Paragraph(f)); }
                }
                TagEnd::Heading(_) => {
                    let f = std::mem::take(&mut self.frags);
                    self.blocks.push(Block::Heading(self.heading, f));
                    self.heading = 0;
                }
                TagEnd::Strong => self.bold = false,
                TagEnd::Emphasis => self.italic = false,
                TagEnd::Strikethrough => self.strike = false,
                TagEnd::CodeBlock => {
                    let code = std::mem::take(&mut self.code_buf);
                    let lang = std::mem::take(&mut self.code_lang);
                    self.blocks.push(Block::CodeBlock(lang, code));
                    self.in_code = false;
                }
                TagEnd::List(_) => { self.list_kind.pop(); self.list_idx.pop(); }
                TagEnd::Item => {
                    let kind = self.list_kind.last().copied().unwrap_or(ListKind::Bullet);
                    let idx = self.list_idx.last().copied().unwrap_or(0);
                    let f = std::mem::take(&mut self.frags);
                    self.blocks.push(Block::ListItem(kind, idx, f));
                }
                TagEnd::Link => {} // Already rendered inline
                _ => {}
            },
            Event::Rule => { self.blocks.push(Block::Rule); }
            _ => {}
        }
    }

    fn flush(self, ui: &mut egui::Ui) {
        for block in &self.blocks {
            match block {
                Block::Paragraph(frags) => {
                    ui.horizontal_wrapped(|ui| {
                        for f in frags {
                            ui.label(f.clone().color(colors::TEXT_PRIMARY));
                        }
                    });
                    ui.add_space(4.0);
                }
                Block::Heading(level, frags) => {
                    let sp = match level { 1 => 12.0, 2 => 10.0, _ => 8.0 };
                    ui.add_space(sp);
                    ui.horizontal_wrapped(|ui| {
                        for f in frags {
                            ui.label(f.clone().color(colors::TEXT_PRIMARY));
                        }
                    });
                    ui.add_space(sp * 0.5);
                }
                Block::CodeBlock(lang, code) => {
                    ui.add_space(4.0);
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(22, 24, 32))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(42, 44, 56)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            if !lang.is_empty() {
                                ui.colored_label(
                                    colors::TEXT_MUTED,
                                    RichText::new(lang.as_str()).size(10.0),
                                );
                                ui.add_space(2.0);
                            }
                            for line in code.lines() {
                                ui.colored_label(
                                    Color32::from_rgb(200, 210, 220),
                                    RichText::new(line)
                                        .size(12.0)
                                        .family(egui::FontFamily::Monospace),
                                );
                            }
                        });
                    ui.add_space(4.0);
                }
                Block::Rule => {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                Block::ListItem(kind, idx, frags) => {
                    ui.horizontal_wrapped(|ui| {
                        let bullet = match kind {
                            ListKind::Bullet => "• ".to_string(),
                            ListKind::Numbered => format!("{}. ", idx),
                        };
                        ui.label(RichText::new(&bullet).size(13.5).color(colors::TEXT_MUTED));
                        for f in frags {
                            ui.label(f.clone().color(colors::TEXT_PRIMARY));
                        }
                    });
                }
            }
        }
    }
}