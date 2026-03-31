use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthStr;

use crate::parser::{inlines_to_plain_text, DeckSettings, InlineContent, Slide, SlideElement};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

pub struct SlideRenderer {
    width: usize,
    height: usize,
    settings: DeckSettings,
    total_slides: usize,
}

#[allow(dead_code)]
struct ThemeColors {
    title: Color,
    heading2: Color,
    heading_other: Color,
    text: Color,
    code_bg: Color,
    code_fg: Color,
    inline_code_bg: Color,
    inline_code_fg: Color,
    link: Color,
    blockquote: Color,
    hr: Color,
    dim: Color,
    slide_bg: Color,
    strikethrough: Color,
}

fn resolve_theme_colors(slide: &Slide, settings: &DeckSettings) -> ThemeColors {
    let is_invert =
        slide.class.iter().any(|c| c == "invert") || settings.class.iter().any(|c| c == "invert");

    let text_color = slide
        .color
        .as_deref()
        .and_then(parse_css_color)
        .unwrap_or_else(|| {
            if is_invert {
                Color::Rgb(30, 30, 30)
            } else {
                Color::White
            }
        });

    let slide_bg = slide
        .background_color
        .as_deref()
        .and_then(parse_css_color)
        .unwrap_or_else(|| {
            if is_invert {
                Color::Rgb(230, 230, 230)
            } else {
                Color::Rgb(26, 26, 46)
            }
        });

    if is_invert {
        ThemeColors {
            title: Color::Rgb(200, 100, 0),
            heading2: Color::Rgb(30, 100, 170),
            heading_other: text_color,
            text: text_color,
            code_bg: Color::Rgb(200, 200, 210),
            code_fg: Color::Rgb(30, 30, 30),
            inline_code_bg: Color::Rgb(200, 200, 210),
            inline_code_fg: Color::Rgb(30, 30, 30),
            link: Color::Rgb(30, 100, 170),
            blockquote: Color::Rgb(100, 100, 100),
            hr: Color::Rgb(180, 180, 180),
            dim: Color::Rgb(100, 100, 100),
            slide_bg,
            strikethrough: Color::Rgb(130, 130, 130),
        }
    } else {
        ThemeColors {
            title: Color::Rgb(243, 156, 18),
            heading2: Color::Rgb(52, 152, 219),
            heading_other: text_color,
            text: text_color,
            code_bg: Color::Rgb(13, 13, 26),
            code_fg: Color::Rgb(200, 200, 200),
            inline_code_bg: Color::Rgb(50, 50, 70),
            inline_code_fg: Color::Rgb(220, 180, 120),
            link: Color::Rgb(52, 152, 219),
            blockquote: Color::Rgb(150, 150, 150),
            hr: Color::Rgb(60, 60, 60),
            dim: Color::Rgb(100, 100, 100),
            slide_bg,
            strikethrough: Color::Rgb(130, 130, 130),
        }
    }
}

impl SlideRenderer {
    pub fn new(width: usize, height: usize, settings: DeckSettings, total_slides: usize) -> Self {
        Self {
            width,
            height,
            settings,
            total_slides,
        }
    }

    pub fn render(&self, slide: &Slide) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let theme = resolve_theme_colors(slide, &self.settings);
        let is_lead = slide.class.iter().any(|c| c == "lead");

        let has_title = slide.title.is_some();
        let has_content = !slide.content.is_empty();

        if has_title && !has_content {
            self.render_title_slide(slide, &theme, &mut lines);
        } else if is_lead {
            self.render_lead_slide(slide, &theme, &mut lines);
        } else {
            self.render_content_slide(slide, &theme, &mut lines);
        }

        if let Some(ref header) = slide.header {
            if !header.is_empty() && !lines.is_empty() {
                let header_style = Style::default().fg(theme.dim);
                let pad = (self.width.saturating_sub(header.len())) / 2;
                lines[0] = Line::from(Span::styled(
                    format!("{}{}", " ".repeat(pad), header),
                    header_style,
                ));
            }
        }

        let total_lines = self.height.saturating_sub(2);
        while lines.len() < total_lines {
            lines.push(Line::from(""));
        }

        if self.settings.paginate {
            let page_str = format!("{} / {}", slide.index + 1, self.total_slide_count_hint());
            if let Some(last) = lines.last_mut() {
                let pad = self.width.saturating_sub(page_str.len() + 2);
                *last = Line::from(Span::styled(
                    format!("{}{}", " ".repeat(pad), page_str),
                    Style::default().fg(theme.dim),
                ));
            }
        }

        if let Some(ref footer) = slide.footer {
            if !footer.is_empty() {
                if let Some(last_line) = lines.last_mut() {
                    let footer_style = Style::default().fg(theme.dim);
                    *last_line = Line::from(Span::styled(format!("  {}", footer), footer_style));
                }
            }
        }

        lines
    }

    fn total_slide_count_hint(&self) -> usize {
        self.total_slides
    }

    fn render_title_slide(
        &self,
        slide: &Slide,
        theme: &ThemeColors,
        lines: &mut Vec<Line<'static>>,
    ) {
        let center_y = self.height / 2;

        for _ in 0..center_y.saturating_sub(2) {
            lines.push(Line::from(""));
        }

        let title_style = Style::default()
            .fg(theme.title)
            .add_modifier(Modifier::BOLD);

        if slide.fit_title {
            let title_text = slide.title.as_deref().unwrap_or("Untitled");
            let available = self.width.saturating_sub(4);
            let display = if title_text.len() > available {
                &title_text[..available]
            } else {
                title_text
            };
            let padding = (self.width.saturating_sub(display.len())) / 2;
            lines.push(Line::from(Span::styled(
                format!("{}{}", " ".repeat(padding), display),
                title_style,
            )));
        } else if !slide.title_inlines.is_empty() {
            let spans = self.render_inlines_styled(&slide.title_inlines, title_style, theme);
            let plain = inlines_to_plain_text(&slide.title_inlines);
            let padding = (self.width.saturating_sub(plain.len())) / 2;
            let mut padded_spans = vec![Span::raw(" ".repeat(padding))];
            padded_spans.extend(spans);
            lines.push(Line::from(padded_spans));
        } else {
            let title = slide.title.as_deref().unwrap_or("Untitled");
            let padding = (self.width.saturating_sub(title.len())) / 2;
            lines.push(Line::from(Span::styled(
                format!("{}{}", " ".repeat(padding), title),
                title_style,
            )));
        }

        for elem in &slide.content {
            match elem {
                SlideElement::Paragraph(inlines) => {
                    let plain = inlines_to_plain_text(inlines);
                    let wrapped = self.wrap_text(&plain, self.width.saturating_sub(4));
                    for line in &wrapped {
                        let pl = (self.width.saturating_sub(line.len())) / 2;
                        lines.push(Line::from(format!("{}{}", " ".repeat(pl), line)));
                    }
                }
                SlideElement::Plain(inlines) => {
                    let plain = inlines_to_plain_text(inlines);
                    let pl = (self.width.saturating_sub(plain.len())) / 2;
                    lines.push(Line::from(format!("{}{}", " ".repeat(pl), plain)));
                }
                SlideElement::BulletList(items) => {
                    for item in items {
                        let text = inlines_to_plain_text(item);
                        let item_str = format!("  • {}", text);
                        let pl = (self.width.saturating_sub(item_str.len())) / 2;
                        lines.push(Line::from(format!("{}{}", " ".repeat(pl), item_str)));
                    }
                }
                _ => {}
            }
        }
    }

    fn render_lead_slide(
        &self,
        slide: &Slide,
        theme: &ThemeColors,
        lines: &mut Vec<Line<'static>>,
    ) {
        let mut content_lines: Vec<Line<'static>> = Vec::new();

        if !slide.title_inlines.is_empty() {
            let title_style = Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD);
            let spans = self.render_inlines_styled(&slide.title_inlines, title_style, theme);
            let plain = inlines_to_plain_text(&slide.title_inlines);
            let padding = (self.width.saturating_sub(plain.len())) / 2;
            let mut padded = vec![Span::raw(" ".repeat(padding))];
            padded.extend(spans);
            content_lines.push(Line::from(padded));
            content_lines.push(Line::from(""));
        }

        for element in &slide.content {
            self.render_element_centered(element, theme, &mut content_lines);
        }

        let start_y = self.height.saturating_sub(content_lines.len()) / 2;
        for _ in 0..start_y {
            lines.push(Line::from(""));
        }
        lines.extend(content_lines);
    }

    fn render_element_centered(
        &self,
        element: &SlideElement,
        theme: &ThemeColors,
        lines: &mut Vec<Line<'static>>,
    ) {
        match element {
            SlideElement::Paragraph(inlines) | SlideElement::Plain(inlines) => {
                let plain = inlines_to_plain_text(inlines);
                let wrapped = self.wrap_text(&plain, self.width.saturating_sub(4));
                for line in wrapped {
                    let pad = (self.width.saturating_sub(line.len())) / 2;
                    lines.push(Line::from(format!("{}{}", " ".repeat(pad), line)));
                }
            }
            SlideElement::BulletList(items) => {
                for item in items {
                    let text = inlines_to_plain_text(item);
                    let item_str = format!("• {}", text);
                    let pad = (self.width.saturating_sub(item_str.len())) / 2;
                    lines.push(Line::from(format!("{}{}", " ".repeat(pad), item_str)));
                }
            }
            _ => {
                let mut tmp = Vec::new();
                let mut y = 0;
                self.render_element(element, theme, &mut tmp, &mut y);
                let max_width = tmp
                    .iter()
                    .map(|l| {
                        l.spans
                            .iter()
                            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                            .sum::<usize>()
                    })
                    .max()
                    .unwrap_or(0);
                let pad = self.width.saturating_sub(max_width) / 2;
                for line in tmp {
                    let mut centered = vec![Span::raw(" ".repeat(pad))];
                    centered.extend(
                        line.spans
                            .into_iter()
                            .map(|s| Span::styled(s.content.into_owned(), s.style)),
                    );
                    lines.push(Line::from(centered));
                }
            }
        }
    }

    fn render_content_slide(
        &self,
        slide: &Slide,
        theme: &ThemeColors,
        lines: &mut Vec<Line<'static>>,
    ) {
        let mut y = 1;

        if !slide.title_inlines.is_empty() {
            let title_style = Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD);
            let spans = self.render_inlines_styled(&slide.title_inlines, title_style, theme);
            let mut padded = vec![Span::raw("  ".to_string())];
            padded.extend(spans);
            lines.push(Line::from(padded));
            lines.push(Line::from(""));
            y += 2;
        } else if let Some(ref title) = slide.title {
            let title_style = Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD);
            lines.push(Line::from(Span::styled(
                format!("  {}", title),
                title_style,
            )));
            lines.push(Line::from(""));
            y += 2;
        }

        for element in &slide.content {
            self.render_element(element, theme, lines, &mut y);
        }
    }

    fn render_element(
        &self,
        element: &SlideElement,
        theme: &ThemeColors,
        lines: &mut Vec<Line<'static>>,
        y: &mut usize,
    ) {
        match element {
            SlideElement::Heading(level, inlines) => {
                let color = match level {
                    1 => theme.title,
                    2 => theme.heading2,
                    _ => theme.heading_other,
                };
                let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                let prefix = match level {
                    1 => "# ",
                    2 => "## ",
                    3 => "### ",
                    _ => "",
                };
                let mut spans = vec![Span::styled(prefix.to_string(), style)];
                spans.extend(self.render_inlines_styled(inlines, style, theme));
                lines.push(Line::from(spans));
                *y += 1;
            }
            SlideElement::Paragraph(inlines) => {
                let rendered_lines = self.render_inlines_wrapped(inlines, theme);
                for line in rendered_lines {
                    lines.push(line);
                    *y += 1;
                }
            }
            SlideElement::CodeBlock(lang, code) => {
                if lang == "ascii" || lang == "art" {
                    for line in code.lines() {
                        lines.push(Line::from(line.trim_end().to_string()));
                        *y += 1;
                    }
                } else {
                    let highlighted = self.highlight_code(code, lang);
                    for line in highlighted {
                        let mut spans = vec![Span::raw("    ".to_string())];
                        spans.extend(line);
                        lines.push(Line::from(spans));
                        *y += 1;
                    }
                }
                *y += 1;
            }
            SlideElement::BulletList(items) => {
                for item in items {
                    let mut spans = vec![Span::raw("  • ".to_string())];
                    spans.extend(self.render_inlines_flat(item, theme));
                    lines.push(Line::from(spans));
                    *y += 1;
                }
                *y += 1;
            }
            SlideElement::NumberedList(start, items) => {
                for (i, item) in items.iter().enumerate() {
                    let num = *start as usize + i;
                    let mut spans = vec![Span::raw(format!("  {}. ", num))];
                    spans.extend(self.render_inlines_flat(item, theme));
                    lines.push(Line::from(spans));
                    *y += 1;
                }
                *y += 1;
            }
            SlideElement::Blockquote(inlines) => {
                let style = Style::default()
                    .fg(theme.blockquote)
                    .add_modifier(Modifier::ITALIC);
                let bar = Span::styled("  │ ".to_string(), style);
                let content_spans = self.render_inlines_styled(inlines, style, theme);
                let mut spans = vec![bar];
                spans.extend(content_spans);
                lines.push(Line::from(spans));
                *y += 1;
            }
            SlideElement::HorizontalRule => {
                let hr = "─".repeat(self.width.saturating_sub(4));
                lines.push(Line::from(Span::styled(
                    format!("  {}", hr),
                    Style::default().fg(theme.hr),
                )));
                *y += 1;
            }
            SlideElement::Table(rows) => {
                if rows.is_empty() {
                    return;
                }
                let num_cols = rows[0].len();
                let mut col_widths: Vec<usize> = vec![0; num_cols];
                for row in rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i < num_cols {
                            let cell_spans = self.render_inlines_flat(cell, theme);
                            let rendered_width: usize = cell_spans
                                .iter()
                                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                                .sum();
                            col_widths[i] = col_widths[i].max(rendered_width);
                        }
                    }
                }
                for (row_idx, row) in rows.iter().enumerate() {
                    let mut spans = vec![Span::raw("  ".to_string())];
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            spans.push(Span::styled(
                                " │ ".to_string(),
                                Style::default().fg(theme.dim),
                            ));
                        }
                        let cell_spans = self.render_inlines_flat(cell, theme);
                        let rendered_width: usize = cell_spans
                            .iter()
                            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                            .sum();
                        let w = col_widths.get(i).copied().unwrap_or(0);
                        spans.extend(cell_spans);
                        if rendered_width < w {
                            spans.push(Span::raw(" ".repeat(w - rendered_width)));
                        }
                    }
                    lines.push(Line::from(spans));
                    *y += 1;
                    if row_idx == 0 && rows.len() > 1 {
                        let mut sep_spans = vec![Span::raw("  ".to_string())];
                        for (i, w) in col_widths.iter().enumerate() {
                            if i > 0 {
                                sep_spans.push(Span::styled(
                                    "─┼─".to_string(),
                                    Style::default().fg(theme.dim),
                                ));
                            }
                            sep_spans
                                .push(Span::styled("─".repeat(*w), Style::default().fg(theme.dim)));
                        }
                        lines.push(Line::from(sep_spans));
                        *y += 1;
                    }
                }
            }
            SlideElement::Plain(inlines) => {
                let rendered_lines = self.render_inlines_wrapped(inlines, theme);
                for line in rendered_lines {
                    lines.push(line);
                    *y += 1;
                }
            }
            SlideElement::Image(img) => {
                let mut desc = format!("[Image: {}", img.url);
                if let Some(ref w) = img.width {
                    desc.push_str(&format!(" w:{}", w));
                }
                if let Some(ref h) = img.height {
                    desc.push_str(&format!(" h:{}", h));
                }
                desc.push(']');
                lines.push(Line::from(Span::styled(
                    format!("  {}", desc),
                    Style::default().fg(theme.dim),
                )));
                *y += 1;
            }
            SlideElement::ColumnBreak => {}
        }
    }

    fn render_inlines_flat(
        &self,
        inlines: &[InlineContent],
        theme: &ThemeColors,
    ) -> Vec<Span<'static>> {
        let base_style = Style::default().fg(theme.text);
        self.render_inlines_styled(inlines, base_style, theme)
    }

    fn render_inlines_styled(
        &self,
        inlines: &[InlineContent],
        base_style: Style,
        theme: &ThemeColors,
    ) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for inline in inlines {
            self.render_inline_recursive(inline, base_style, theme, &mut spans);
        }
        spans
    }

    fn render_inline_recursive(
        &self,
        inline: &InlineContent,
        style: Style,
        theme: &ThemeColors,
        spans: &mut Vec<Span<'static>>,
    ) {
        match inline {
            InlineContent::Text(text) => {
                spans.push(Span::styled(text.clone(), style));
            }
            InlineContent::Bold(children) => {
                let bold_style = style.add_modifier(Modifier::BOLD);
                for child in children {
                    self.render_inline_recursive(child, bold_style, theme, spans);
                }
            }
            InlineContent::Italic(children) => {
                let italic_style = style.add_modifier(Modifier::ITALIC);
                for child in children {
                    self.render_inline_recursive(child, italic_style, theme, spans);
                }
            }
            InlineContent::Strikethrough(children) => {
                let strike_style = style
                    .fg(theme.strikethrough)
                    .add_modifier(Modifier::CROSSED_OUT);
                for child in children {
                    self.render_inline_recursive(child, strike_style, theme, spans);
                }
            }
            InlineContent::Code(text) => {
                let code_style = Style::default()
                    .fg(theme.inline_code_fg)
                    .bg(theme.inline_code_bg);
                spans.push(Span::styled(format!(" {} ", text), code_style));
            }
            InlineContent::Link { text, url } => {
                let link_style = style.fg(theme.link).add_modifier(Modifier::UNDERLINED);
                for child in text {
                    self.render_inline_recursive(child, link_style, theme, spans);
                }
                if !url.is_empty() && !url.starts_with('#') {
                    spans.push(Span::styled(
                        format!(" ({})", url),
                        Style::default().fg(theme.dim),
                    ));
                }
            }
            InlineContent::LineBreak => {
                spans.push(Span::raw("\n".to_string()));
            }
        }
    }

    fn render_inlines_wrapped(
        &self,
        inlines: &[InlineContent],
        theme: &ThemeColors,
    ) -> Vec<Line<'static>> {
        let plain = inlines_to_plain_text(inlines);
        let max_width = self.width.saturating_sub(4);
        let wrapped = self.wrap_text(&plain, max_width);

        if wrapped.len() <= 1 {
            let spans = self.render_inlines_flat(inlines, theme);
            return vec![Line::from(spans)];
        }

        wrapped
            .into_iter()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(theme.text))))
            .collect()
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

    fn highlight_code(&self, code: &str, lang: &str) -> Vec<Vec<Span<'static>>> {
        let syntax_set = SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines);
        let theme_set = THEME_SET.get_or_init(ThemeSet::load_defaults);

        let syntax = syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let theme = &theme_set.themes["base16-ocean.dark"];

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in code.lines() {
            let ranges = highlighter
                .highlight_line(line, syntax_set)
                .unwrap_or_default();

            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(syntect_style, text)| {
                    let fg = Color::Rgb(
                        syntect_style.foreground.r,
                        syntect_style.foreground.g,
                        syntect_style.foreground.b,
                    );
                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            result.push(spans);
        }

        result
    }
}

fn parse_css_color(color: &str) -> Option<Color> {
    let trimmed = color.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        let hex = hex.trim();
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    if trimmed.starts_with("rgb(") && trimmed.ends_with(')') {
        let inner = &trimmed[4..trimmed.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    match trimmed.to_lowercase().as_str() {
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "blue" => Some(Color::Blue),
        "yellow" => Some(Color::Yellow),
        "cyan" => Some(Color::Cyan),
        "magenta" => Some(Color::Magenta),
        "gray" | "grey" => Some(Color::Gray),
        _ => None,
    }
}
