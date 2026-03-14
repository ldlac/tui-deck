use ratatui::prelude::Stylize;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::Block,
};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::parser::{Slide, SlideElement, SlideType};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

pub struct SlideRenderer {
    width: usize,
    height: usize,
}

impl SlideRenderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    pub fn render(&self, slide: &Slide) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        match slide.slide_type {
            SlideType::Title => self.render_title_slide(slide, &mut lines),
            _ => self.render_content_slide(slide, &mut lines),
        }

        // Pad to fill screen
        let total_lines = self.height.saturating_sub(2);
        while lines.len() < total_lines {
            lines.push(Line::from(""));
        }

        lines
    }

    fn render_title_slide(&self, slide: &Slide, lines: &mut Vec<Line<'static>>) {
        let center_y = self.height / 2;
        let title = slide.title.as_deref().unwrap_or("Untitled");

        for _ in 0..center_y.saturating_sub(1) {
            lines.push(Line::from(""));
        }

        let title_style = Style::default()
            .fg(Color::Rgb(243, 156, 18)) // Amber accent
            .bold();

        let padding = (self.width.saturating_sub(title.len())) / 2;
        let padded_title = format!("{}{}", " ".repeat(padding), title);
        lines.push(Line::from(Span::styled(padded_title, title_style)));

        if let Some(ref notes) = slide.notes {
            for _ in 0..1 {
                lines.push(Line::from(""));
            }
            let notes_style = Style::default().fg(Color::Rgb(100, 100, 100));
            let padded_notes = format!("{}{}", " ".repeat(padding), notes);
            lines.push(Line::from(Span::styled(padded_notes, notes_style)));
        }
    }

    fn render_content_slide(&self, slide: &Slide, lines: &mut Vec<Line<'static>>) {
        let mut y = 1;

        if let Some(ref title) = slide.title {
            let title_style = Style::default().fg(Color::Rgb(243, 156, 18)).bold();
            let padding = self.width.saturating_sub(title.len() + 4);
            let padded = format!("  {}{}", title, " ".repeat(padding));
            lines.push(Line::from(Span::styled(padded, title_style)));
            lines.push(Line::from(""));
            y += 2;
        }

        for element in &slide.content {
            self.render_element(element, lines, &mut y);
        }
    }

    fn render_element(
        &self,
        element: &SlideElement,
        lines: &mut Vec<Line<'static>>,
        y: &mut usize,
    ) {
        match element {
            SlideElement::Heading(level, text) => {
                let color = match level {
                    1 => Color::Rgb(243, 156, 18),
                    2 => Color::Rgb(52, 152, 219),
                    _ => Color::White,
                };
                let style = Style::default().fg(color).bold();
                let prefix = match level {
                    1 => "# ",
                    2 => "## ",
                    3 => "### ",
                    _ => "",
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, text),
                    style,
                )));
                *y += 1;
            }
            SlideElement::Paragraph(text) => {
                let wrapped = self.wrap_text(text, self.width.saturating_sub(4));
                for line in wrapped {
                    lines.push(Line::from(line));
                    *y += 1;
                }
            }
            SlideElement::CodeBlock(lang, code) => {
                if lang == "ascii" || lang == "art" {
                    for line in code.lines() {
                        let trimmed = line.trim_end();
                        lines.push(Line::from(trimmed.to_string()));
                        *y += 1;
                    }
                } else {
                    let highlighted = self.highlight_code(code, lang);
                    let code_style = Style::default().bg(Color::Rgb(13, 13, 26));

                    for line in highlighted {
                        let padded = format!("    {}", line);
                        lines.push(Line::from(Span::styled(padded, code_style)));
                        *y += 1;
                    }
                }
                *y += 1;
            }
            SlideElement::BulletList(items) => {
                for item in items {
                    let styled = format!("  • {}", item);
                    lines.push(Line::from(styled));
                    *y += 1;
                }
                *y += 1;
            }
            SlideElement::NumberedList(items) => {
                for (i, item) in items.iter().enumerate() {
                    let styled = format!("  {}. {}", i + 1, item);
                    lines.push(Line::from(styled));
                    *y += 1;
                }
                *y += 1;
            }
            SlideElement::Blockquote(text) => {
                let style = Style::default().fg(Color::Rgb(100, 100, 100)).italic();
                lines.push(Line::from(Span::styled(format!("  │ {}", text), style)));
                *y += 1;
            }
            SlideElement::HorizontalRule => {
                let hr = "─".repeat(self.width.saturating_sub(4));
                lines.push(Line::from(Span::styled(
                    hr,
                    Style::default().fg(Color::Rgb(50, 50, 50)),
                )));
                *y += 1;
            }
            SlideElement::Plain(text) => {
                let wrapped = self.wrap_text(text, self.width.saturating_sub(4));
                for line in wrapped {
                    lines.push(Line::from(line));
                    *y += 1;
                }
            }
            SlideElement::Image(img) => {
                let img_text = format!("[Image: {}]", img.url);
                lines.push(Line::from(img_text));
                *y += 1;
            }
            SlideElement::ColumnBreak | SlideElement::Image(_) => {}
        }
    }

    fn wrap_text(&self, text: &str, max_width: usize) -> Vec<String> {
        if max_width == 0 {
            return vec![text.to_string()];
        }

        let mut result = Vec::new();
        for paragraph in text.split("\n\n") {
            let mut line = String::new();
            for word in paragraph.split_whitespace() {
                if line.is_empty() {
                    line = word.to_string();
                } else if line.len() + 1 + word.len() <= max_width {
                    line.push(' ');
                    line.push_str(word);
                } else {
                    result.push(line);
                    line = word.to_string();
                }
            }
            if !line.is_empty() {
                result.push(line);
            }
        }
        if result.is_empty() {
            result.push(String::new());
        }
        result
    }

    fn highlight_code(&self, code: &str, lang: &str) -> Vec<String> {
        use syntect::highlighting::Style as SyntectStyle;

        let syntax_set = SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines);
        let theme_set = THEME_SET.get_or_init(ThemeSet::load_defaults);

        let syntax = syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let theme = &theme_set.themes["base16-ocean.dark"];

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in code.lines() {
            let ranges: Vec<(SyntectStyle, &str)> = highlighter
                .highlight_line(line, syntax_set)
                .unwrap_or_default();

            let mut styled_line = String::new();
            for (_, text) in ranges {
                styled_line.push_str(text);
            }
            result.push(styled_line);
        }

        result
    }
}
