use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub index: usize,
    pub title: Option<String>,
    pub title_inlines: Vec<InlineContent>,
    pub content: Vec<SlideElement>,
    pub notes: Option<String>,
    pub slide_type: SlideType,
    pub class: Vec<String>,
    pub background_color: Option<String>,
    pub background_image: Option<String>,
    pub color: Option<String>,
    pub image: Option<ImageSpec>,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub fit_title: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSpec {
    pub url: String,
    pub width: Option<String>,
    pub height: Option<String>,
    pub position: Option<String>,
    pub is_background: bool,
    pub bg_size: Option<String>,
    pub bg_direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SlideType {
    Title,
    Content,
    Code,
    AsciiArt,
    Split,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckSettings {
    pub theme: Option<String>,
    pub paginate: bool,
    pub class: Vec<String>,
    pub background_color: Option<String>,
    pub color: Option<String>,
    pub size: Option<String>,
    pub heading_divider: Option<Vec<usize>>,
    pub header: Option<String>,
    pub footer: Option<String>,
}

impl Default for DeckSettings {
    fn default() -> Self {
        Self {
            theme: None,
            paginate: false,
            class: Vec::new(),
            background_color: None,
            color: None,
            size: None,
            heading_divider: None,
            header: None,
            footer: None,
        }
    }
}

/// Rich inline content that preserves formatting information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InlineContent {
    Text(String),
    Bold(Vec<InlineContent>),
    Italic(Vec<InlineContent>),
    Strikethrough(Vec<InlineContent>),
    Code(String),
    Link {
        text: Vec<InlineContent>,
        url: String,
    },
    LineBreak,
}

impl InlineContent {
    /// Extract plain text content, stripping all formatting.
    pub fn plain_text(&self) -> String {
        match self {
            InlineContent::Text(t) => t.clone(),
            InlineContent::Bold(children)
            | InlineContent::Italic(children)
            | InlineContent::Strikethrough(children) => {
                children.iter().map(|c| c.plain_text()).collect()
            }
            InlineContent::Code(t) => t.clone(),
            InlineContent::Link { text, .. } => text.iter().map(|c| c.plain_text()).collect(),
            InlineContent::LineBreak => "\n".to_string(),
        }
    }
}

/// Helper to get plain text from a Vec<InlineContent>.
pub fn inlines_to_plain_text(inlines: &[InlineContent]) -> String {
    inlines.iter().map(|i| i.plain_text()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlideElement {
    Heading(usize, Vec<InlineContent>),
    Paragraph(Vec<InlineContent>),
    CodeBlock(String, String),
    BulletList(Vec<Vec<InlineContent>>),
    NumberedList(u64, Vec<Vec<InlineContent>>),
    Blockquote(Vec<InlineContent>),
    HorizontalRule,
    Plain(Vec<InlineContent>),
    Image(ImageSpec),
    ColumnBreak,
    Table(Vec<Vec<Vec<InlineContent>>>),
}

/// Stack frame for tracking nested inline formatting context.
#[derive(Debug)]
enum InlineFrame {
    Emphasis,
    Strong,
    Strikethrough,
    Link(String),
}

pub struct MarkdownParser {
    slides: Vec<Slide>,
    current_slide: Option<Slide>,
    current_elements: Vec<SlideElement>,
    inline_stack: Vec<(InlineFrame, Vec<InlineContent>)>,
    current_inlines: Vec<InlineContent>,
    current_list: Option<Vec<Vec<InlineContent>>>,
    list_type: Option<ListType>,
    code_buffer: Option<(String, String)>,
    in_code_block: bool,
    in_ascii_block: bool,
    in_presenter_notes: bool,
    notes_buffer: String,
    settings: DeckSettings,
    pending_class: Option<Vec<String>>,
    in_table: bool,
    table_rows: Vec<Vec<Vec<InlineContent>>>,
    current_table_row: Vec<Vec<InlineContent>>,
    in_table_cell: bool,
    in_table_head: bool,
    table_head_row: Vec<Vec<InlineContent>>,
    in_heading: bool,
    heading_level: usize,
    heading_inlines: Vec<InlineContent>,
    in_blockquote: bool,
    blockquote_inlines: Vec<InlineContent>,
    in_list_item: bool,
    in_paragraph: bool,
    check_fit_comment: bool,
}

#[derive(Clone, Debug)]
enum ListType {
    Bullet,
    Numbered(u64),
}

impl MarkdownParser {
    pub fn new() -> Self {
        Self {
            slides: Vec::new(),
            current_slide: None,
            current_elements: Vec::new(),
            inline_stack: Vec::new(),
            current_inlines: Vec::new(),
            current_list: None,
            list_type: None,
            code_buffer: None,
            in_code_block: false,
            in_ascii_block: false,
            in_presenter_notes: false,
            notes_buffer: String::new(),
            settings: DeckSettings::default(),
            pending_class: None,
            in_table: false,
            table_rows: Vec::new(),
            current_table_row: Vec::new(),
            in_table_cell: false,
            in_table_head: false,
            table_head_row: Vec::new(),
            in_heading: false,
            heading_level: 0,
            heading_inlines: Vec::new(),
            in_blockquote: false,
            blockquote_inlines: Vec::new(),
            in_list_item: false,
            in_paragraph: false,
            check_fit_comment: false,
        }
    }

    pub fn parse(&mut self, markdown: &str) -> (Vec<Slide>, DeckSettings) {
        let markdown = self.strip_front_matter(markdown);
        let parser = Parser::new_ext(&markdown, Options::all());
        let events: Vec<Event> = parser.collect();

        for event in events {
            self.process_event(event);
        }

        self.finish_current_slide();

        if self.slides.is_empty() {
            self.slides.push(Slide {
                index: 0,
                title: None,
                title_inlines: Vec::new(),
                content: vec![SlideElement::Plain(vec![InlineContent::Text(
                    "No content".to_string(),
                )])],
                notes: None,
                slide_type: SlideType::Content,
                class: Vec::new(),
                background_color: None,
                background_image: None,
                color: None,
                image: None,
                header: None,
                footer: None,
                fit_title: false,
            });
        }

        for (i, slide) in self.slides.iter_mut().enumerate() {
            slide.index = i;
            if slide.background_color.is_none() {
                slide.background_color = self.settings.background_color.clone();
            }
            if slide.color.is_none() {
                slide.color = self.settings.color.clone();
            }
            if slide.header.is_none() {
                slide.header = self.settings.header.clone();
            }
            if slide.footer.is_none() {
                slide.footer = self.settings.footer.clone();
            }
            if slide.class.is_empty() && !self.settings.class.is_empty() {
                slide.class = self.settings.class.clone();
            }
            if slide.slide_type == SlideType::Content {
                slide.slide_type = determine_slide_type(slide);
            }
        }

        (self.slides.clone(), self.settings.clone())
    }

    fn strip_front_matter(&mut self, markdown: &str) -> String {
        let trimmed = markdown.trim_start();
        if trimmed.starts_with("---") {
            if let Some(end) = trimmed[3..].find("---") {
                let yaml_content = &trimmed[3..3 + end];
                self.parse_front_matter(yaml_content);
                return trimmed[3 + end + 3..].trim_start().to_string();
            }
        }
        markdown.to_string()
    }

    fn parse_front_matter(&mut self, yaml: &str) {
        for line in yaml.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('\'').trim_matches('"');
                match key {
                    "theme" => self.settings.theme = Some(value.to_string()),
                    "paginate" => self.settings.paginate = value == "true",
                    "class" => {
                        self.settings.class =
                            value.split(',').map(|s| s.trim().to_string()).collect()
                    }
                    "backgroundColor" | "background_color" => {
                        self.settings.background_color = Some(value.to_string())
                    }
                    "backgroundImage" | "background_image" => {
                        self.settings.background_image_raw(value);
                    }
                    "color" => self.settings.color = Some(value.to_string()),
                    "size" => self.settings.size = Some(value.to_string()),
                    "header" => self.settings.header = Some(value.to_string()),
                    "footer" => self.settings.footer = Some(value.to_string()),
                    "headingDivider" | "heading_divider" => {
                        self.settings.heading_divider = parse_heading_divider(value);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Push current inline content into the appropriate context.
    fn push_inline(&mut self, inline: InlineContent) {
        if self.in_heading {
            self.heading_inlines.push(inline);
        } else if self.in_table_cell {
            self.current_inlines.push(inline);
        } else if self.in_blockquote {
            self.blockquote_inlines.push(inline);
        } else if self.in_list_item {
            self.current_inlines.push(inline);
        } else if self.in_paragraph {
            self.current_inlines.push(inline);
        } else {
            self.current_inlines.push(inline);
        }
    }

    /// Get the current inline buffer for the active context.
    fn active_inlines_mut(&mut self) -> &mut Vec<InlineContent> {
        if self.in_heading {
            &mut self.heading_inlines
        } else if self.in_blockquote {
            &mut self.blockquote_inlines
        } else {
            &mut self.current_inlines
        }
    }

    fn process_event(&mut self, event: Event) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush_list();
                self.in_heading = true;
                self.heading_level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.heading_inlines.clear();
                self.check_fit_comment = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                let level = self.heading_level;
                let inlines = std::mem::take(&mut self.heading_inlines);
                let plain = inlines_to_plain_text(&inlines);
                self.in_heading = false;
                self.check_fit_comment = false;

                let should_split = if let Some(ref dividers) = self.settings.heading_divider {
                    dividers.contains(&level)
                        && self.current_slide.is_some()
                        && (self.current_slide.as_ref().unwrap().title.is_some()
                            || !self.current_elements.is_empty())
                } else {
                    false
                };

                if should_split {
                    self.finish_current_slide();
                    self.current_slide = Some(Slide {
                        index: 0,
                        title: Some(plain.clone()),
                        title_inlines: inlines.clone(),
                        content: Vec::new(),
                        notes: None,
                        slide_type: if level == 1 {
                            SlideType::Title
                        } else {
                            SlideType::Content
                        },
                        class: self.pending_class.take().unwrap_or_default(),
                        background_color: None,
                        background_image: None,
                        color: None,
                        image: None,
                        header: None,
                        footer: None,
                        fit_title: false,
                    });
                } else if let Some(ref mut slide) = self.current_slide {
                    if slide.title.is_none() {
                        slide.title = Some(plain.clone());
                        slide.title_inlines = inlines.clone();
                    } else {
                        self.current_elements
                            .push(SlideElement::Heading(level, inlines));
                    }
                } else {
                    self.current_slide = Some(Slide {
                        index: 0,
                        title: Some(plain.clone()),
                        title_inlines: inlines.clone(),
                        content: Vec::new(),
                        notes: None,
                        slide_type: if level == 1 {
                            SlideType::Title
                        } else {
                            SlideType::Content
                        },
                        class: self.pending_class.take().unwrap_or_default(),
                        background_color: None,
                        background_image: None,
                        color: None,
                        image: None,
                        header: None,
                        footer: None,
                        fit_title: false,
                    });
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush_list();
                self.in_code_block = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.in_ascii_block = lang == "ascii" || lang == "art";
                self.code_buffer = Some((lang, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang, code)) = self.code_buffer.take() {
                    let slide_type = if self.in_ascii_block {
                        SlideType::AsciiArt
                    } else {
                        SlideType::Code
                    };
                    self.current_elements
                        .push(SlideElement::CodeBlock(lang, code));
                    if self.current_slide.is_none() {
                        self.current_slide = Some(Slide {
                            index: 0,
                            title: None,
                            title_inlines: Vec::new(),
                            content: Vec::new(),
                            notes: None,
                            slide_type,
                            class: self.pending_class.take().unwrap_or_default(),
                            background_color: None,
                            background_image: None,
                            color: None,
                            image: None,
                            header: None,
                            footer: None,
                            fit_title: false,
                        });
                    }
                }
                self.in_code_block = false;
                self.in_ascii_block = false;
            }
            Event::Start(Tag::Emphasis) => {
                let current = std::mem::take(self.active_inlines_mut());
                self.inline_stack.push((InlineFrame::Emphasis, current));
            }
            Event::End(TagEnd::Emphasis) => {
                let children = std::mem::take(self.active_inlines_mut());
                if let Some((InlineFrame::Emphasis, mut parent)) = self.inline_stack.pop() {
                    parent.push(InlineContent::Italic(children));
                    *self.active_inlines_mut() = parent;
                }
            }
            Event::Start(Tag::Strong) => {
                let current = std::mem::take(self.active_inlines_mut());
                self.inline_stack.push((InlineFrame::Strong, current));
            }
            Event::End(TagEnd::Strong) => {
                let children = std::mem::take(self.active_inlines_mut());
                if let Some((InlineFrame::Strong, mut parent)) = self.inline_stack.pop() {
                    parent.push(InlineContent::Bold(children));
                    *self.active_inlines_mut() = parent;
                }
            }
            Event::Start(Tag::Strikethrough) => {
                let current = std::mem::take(self.active_inlines_mut());
                self.inline_stack
                    .push((InlineFrame::Strikethrough, current));
            }
            Event::End(TagEnd::Strikethrough) => {
                let children = std::mem::take(self.active_inlines_mut());
                if let Some((InlineFrame::Strikethrough, mut parent)) = self.inline_stack.pop() {
                    parent.push(InlineContent::Strikethrough(children));
                    *self.active_inlines_mut() = parent;
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let current = std::mem::take(self.active_inlines_mut());
                self.inline_stack
                    .push((InlineFrame::Link(dest_url.to_string()), current));
            }
            Event::End(TagEnd::Link) => {
                let children = std::mem::take(self.active_inlines_mut());
                if let Some((InlineFrame::Link(url), mut parent)) = self.inline_stack.pop() {
                    parent.push(InlineContent::Link {
                        text: children,
                        url,
                    });
                    *self.active_inlines_mut() = parent;
                }
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let url = dest_url.to_string();
                let _title = title.to_string();
                let current = std::mem::take(self.active_inlines_mut());
                self.inline_stack.push((InlineFrame::Link(url), current));
            }
            Event::End(TagEnd::Image) => {
                let alt_inlines = std::mem::take(self.active_inlines_mut());
                if let Some((InlineFrame::Link(url), parent)) = self.inline_stack.pop() {
                    let alt_text = inlines_to_plain_text(&alt_inlines);
                    let image_spec = parse_image_spec(&url, &alt_text);
                    if image_spec.is_background {
                        if let Some(ref mut slide) = self.current_slide {
                            slide.background_image = Some(url.clone());
                            slide.image = Some(image_spec);
                        }
                    } else {
                        self.current_elements.push(SlideElement::Image(image_spec));
                    }
                    *self.active_inlines_mut() = parent;
                }
            }
            Event::Code(code) => {
                self.push_inline(InlineContent::Code(code.to_string()));
            }
            Event::SoftBreak => {
                self.push_inline(InlineContent::Text(" ".to_string()));
            }
            Event::HardBreak => {
                self.push_inline(InlineContent::LineBreak);
            }
            Event::Text(text) => {
                let text_str = text.to_string();
                if self.in_presenter_notes {
                    if !self.notes_buffer.is_empty() {
                        self.notes_buffer.push('\n');
                    }
                    self.notes_buffer.push_str(&text_str);
                } else if self.in_code_block || self.in_ascii_block {
                    if let Some((_, ref mut code)) = self.code_buffer {
                        if !code.is_empty() {
                            code.push('\n');
                        }
                        code.push_str(&text_str);
                    }
                } else if self.in_heading
                    || self.in_table_cell
                    || self.in_blockquote
                    || self.in_list_item
                    || self.in_paragraph
                {
                    self.push_inline(InlineContent::Text(text_str));
                } else if let Some(ref mut slide) = self.current_slide {
                    let trimmed = text_str.trim();
                    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                        self.finish_current_slide();
                    } else if slide.title.is_none() && !trimmed.is_empty() {
                        slide.title = Some(trimmed.to_string());
                        slide.title_inlines = vec![InlineContent::Text(trimmed.to_string())];
                    } else if !trimmed.is_empty() {
                        self.current_elements
                            .push(SlideElement::Plain(vec![InlineContent::Text(text_str)]));
                    }
                } else if !text_str.trim().is_empty() {
                    self.current_elements
                        .push(SlideElement::Plain(vec![InlineContent::Text(text_str)]));
                }
            }
            Event::Start(Tag::Paragraph) => {
                self.flush_list();
                self.in_paragraph = true;
                self.current_inlines.clear();
            }
            Event::End(TagEnd::Paragraph) => {
                self.in_paragraph = false;
                let inlines = std::mem::take(&mut self.current_inlines);
                if !inlines.is_empty() {
                    if self.in_blockquote {
                        self.blockquote_inlines.extend(inlines);
                    } else if self.in_list_item {
                        self.current_inlines = inlines;
                    } else {
                        self.current_elements.push(SlideElement::Paragraph(inlines));
                    }
                }
            }
            Event::Start(Tag::List(start)) => {
                self.flush_list();
                self.current_list = Some(Vec::new());
                match start {
                    Some(n) => self.list_type = Some(ListType::Numbered(n)),
                    None => self.list_type = Some(ListType::Bullet),
                }
            }
            Event::End(TagEnd::List(_)) => {
                self.flush_list();
            }
            Event::Start(Tag::Item) => {
                self.in_list_item = true;
                self.current_inlines.clear();
                if self.current_list.is_none() {
                    self.current_list = Some(Vec::new());
                }
            }
            Event::End(TagEnd::Item) => {
                self.in_list_item = false;
                let inlines = std::mem::take(&mut self.current_inlines);
                if let Some(ref mut list) = self.current_list {
                    list.push(inlines);
                }
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked {
                    InlineContent::Text("☑ ".to_string())
                } else {
                    InlineContent::Text("☐ ".to_string())
                };
                self.push_inline(marker);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.flush_list();
                self.in_blockquote = true;
                self.blockquote_inlines.clear();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.in_blockquote = false;
                let inlines = std::mem::take(&mut self.blockquote_inlines);
                if !inlines.is_empty() {
                    self.current_elements
                        .push(SlideElement::Blockquote(inlines));
                }
            }
            Event::Start(Tag::Table(_)) => {
                self.flush_list();
                self.in_table = true;
                self.table_rows.clear();
                self.current_table_row.clear();
                self.table_head_row.clear();
            }
            Event::End(TagEnd::Table) => {
                let mut all_rows = Vec::new();
                if !self.table_head_row.is_empty() {
                    all_rows.push(std::mem::take(&mut self.table_head_row));
                }
                all_rows.append(&mut self.table_rows);
                if !all_rows.is_empty() {
                    self.current_elements.push(SlideElement::Table(all_rows));
                }
                self.in_table = false;
            }
            Event::Start(Tag::TableHead) => {
                self.in_table_head = true;
                self.current_table_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                self.in_table_head = false;
                self.table_head_row = std::mem::take(&mut self.current_table_row);
            }
            Event::Start(Tag::TableRow) => {
                self.current_table_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if !self.in_table_head && !self.current_table_row.is_empty() {
                    self.table_rows
                        .push(std::mem::take(&mut self.current_table_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                self.in_table_cell = true;
                self.current_inlines.clear();
            }
            Event::End(TagEnd::TableCell) => {
                self.in_table_cell = false;
                let inlines = std::mem::take(&mut self.current_inlines);
                self.current_table_row.push(inlines);
            }
            Event::Rule => {
                self.finish_current_slide();
            }
            Event::Html(html) => {
                let html_str = html.to_string();
                self.process_html_comment(&html_str);
            }
            _ => {}
        }
    }

    fn process_html_comment(&mut self, html: &str) {
        let trimmed = html.trim();

        if !trimmed.starts_with("<!--") || !trimmed.ends_with("-->") {
            return;
        }

        let inner = &trimmed[4..trimmed.len() - 3].trim();

        if self.in_heading && *inner == "fit" {
            if let Some(ref mut slide) = self.current_slide {
                slide.fit_title = true;
            }
            return;
        }

        if inner.starts_with("notes:") {
            if let Some(notes) = inner.strip_prefix("notes:") {
                if let Some(ref mut slide) = self.current_slide {
                    slide.notes = Some(notes.trim().to_string());
                }
            }
            return;
        }

        if inner.starts_with("?") && *inner != "?" {
            self.in_presenter_notes = true;
            self.notes_buffer.clear();
            return;
        }

        if *inner == "?" {
            self.in_presenter_notes = false;
            if !self.notes_buffer.is_empty() {
                if let Some(ref mut slide) = self.current_slide {
                    if slide.notes.is_none() {
                        slide.notes = Some(self.notes_buffer.trim().to_string());
                    }
                }
                self.notes_buffer.clear();
            }
            return;
        }

        if let Some(class) = inner.strip_prefix("class:") {
            self.pending_class = Some(
                class
                    .trim()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect(),
            );
            return;
        }

        if let Some(class) = inner.strip_prefix("_class:") {
            if let Some(ref mut slide) = self.current_slide {
                slide
                    .class
                    .extend(class.trim().split(',').map(|s| s.trim().to_string()));
            }
            return;
        }

        if let Some(bg) = inner.strip_prefix("bg:") {
            if let Some(ref mut slide) = self.current_slide {
                slide.background_color = Some(bg.trim().to_string());
            }
            return;
        }

        if let Some(bg) = inner.strip_prefix("backgroundColor:") {
            if let Some(ref mut slide) = self.current_slide {
                slide.background_color = Some(bg.trim().to_string());
            }
            return;
        }

        if let Some(bg) = inner.strip_prefix("backgroundImage:") {
            if let Some(ref mut slide) = self.current_slide {
                slide.background_image = Some(
                    bg.trim()
                        .trim_matches(|c| c == '\'' || c == '"')
                        .to_string(),
                );
            }
            return;
        }

        if let Some(color) = inner.strip_prefix("color:") {
            if let Some(ref mut slide) = self.current_slide {
                slide.color = Some(color.trim().to_string());
            }
            return;
        }

        if let Some(color) = inner.strip_prefix("_color:") {
            if let Some(ref mut slide) = self.current_slide {
                slide.color = Some(color.trim().to_string());
            }
            return;
        }

        if let Some(header) = inner.strip_prefix("header:") {
            let val = header
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            self.settings.header = Some(val);
            return;
        }

        if let Some(header) = inner.strip_prefix("_header:") {
            let val = header
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            if let Some(ref mut slide) = self.current_slide {
                slide.header = Some(val);
            }
            return;
        }

        if let Some(footer) = inner.strip_prefix("footer:") {
            let val = footer
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            self.settings.footer = Some(val);
            return;
        }

        if let Some(footer) = inner.strip_prefix("_footer:") {
            let val = footer
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            if let Some(ref mut slide) = self.current_slide {
                slide.footer = Some(val);
            }
            return;
        }

        if let Some(paginate_val) = inner.strip_prefix("paginate:") {
            let val = paginate_val.trim();
            self.settings.paginate = val == "true";
            return;
        }

        if let Some(paginate_val) = inner.strip_prefix("_paginate:") {
            let val = paginate_val.trim();
            self.settings.paginate = val == "true";
            return;
        }

        if inner.trim() == "---" {
            self.finish_current_slide();
        }
    }

    fn flush_list(&mut self) {
        if let Some(list) = self.current_list.take() {
            if !list.is_empty() {
                let list_elem = match self.list_type {
                    Some(ListType::Bullet) | None => SlideElement::BulletList(list),
                    Some(ListType::Numbered(start)) => SlideElement::NumberedList(start, list),
                };
                self.current_elements.push(list_elem);
            }
        }
        self.list_type = None;
    }

    fn finish_current_slide(&mut self) {
        self.flush_list();

        self.in_presenter_notes = false;
        if !self.notes_buffer.is_empty() {
            if let Some(ref mut slide) = self.current_slide {
                if slide.notes.is_none() {
                    slide.notes = Some(self.notes_buffer.trim().to_string());
                }
            }
            self.notes_buffer.clear();
        }

        if let Some(mut slide) = self.current_slide.take() {
            if !self.current_elements.is_empty() || slide.title.is_some() {
                slide.content = std::mem::take(&mut self.current_elements);

                if slide.slide_type == SlideType::Content {
                    slide.slide_type = determine_slide_type(&slide);
                }

                self.slides.push(slide);
            }
            self.current_elements = Vec::new();
        }
    }
}

impl DeckSettings {
    fn background_image_raw(&mut self, _value: &str) {}
}

fn determine_slide_type(slide: &Slide) -> SlideType {
    let has_code = slide
        .content
        .iter()
        .any(|e| matches!(e, SlideElement::CodeBlock(_, _)));

    if slide.title.is_some() && slide.content.is_empty() {
        return SlideType::Title;
    }

    if has_code {
        for elem in &slide.content {
            if let SlideElement::CodeBlock(ref lang, _) = elem {
                if lang == "ascii" || lang == "art" {
                    return SlideType::AsciiArt;
                }
            }
        }
        return SlideType::Code;
    }

    SlideType::Content
}

/// Parse Marp image sizing directives from alt text.
/// Examples: "bg", "bg fit", "bg left", "width:200px", "w:50% h:300px"
fn parse_image_spec(url: &str, alt_text: &str) -> ImageSpec {
    let mut spec = ImageSpec {
        url: url.to_string(),
        width: None,
        height: None,
        position: None,
        is_background: false,
        bg_size: None,
        bg_direction: None,
    };

    let parts: Vec<&str> = alt_text.split_whitespace().collect();
    for part in &parts {
        let lower = part.to_lowercase();
        if lower == "bg" {
            spec.is_background = true;
        } else if lower == "contain" || lower == "cover" || lower == "fit" || lower == "auto" {
            spec.bg_size = Some(lower);
        } else if lower == "left" || lower == "right" {
            spec.bg_direction = Some(lower);
        } else if lower == "vertical" {
            spec.bg_direction = Some(lower);
        } else if let Some(w) = lower
            .strip_prefix("w:")
            .or_else(|| lower.strip_prefix("width:"))
        {
            spec.width = Some(w.to_string());
        } else if let Some(h) = lower
            .strip_prefix("h:")
            .or_else(|| lower.strip_prefix("height:"))
        {
            spec.height = Some(h.to_string());
        }
    }

    spec
}

/// Parse headingDivider value: can be a single number or array like [1,2]
fn parse_heading_divider(value: &str) -> Option<Vec<usize>> {
    let trimmed = value.trim();
    if let Ok(n) = trimmed.parse::<usize>() {
        return Some((1..=n).collect());
    }
    let inner = trimmed.trim_start_matches('[').trim_end_matches(']');
    let nums: Vec<usize> = inner
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if !nums.is_empty() {
        Some(nums)
    } else {
        None
    }
}

pub fn parse_markdown(markdown: &str) -> (Vec<Slide>, DeckSettings) {
    let mut parser = MarkdownParser::new();
    parser.parse(markdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::SlideRenderer;

    #[test]
    fn parse_and_render_slides_md() {
        let md = include_str!("../slides.md");
        let (slides, settings) = parse_markdown(md);
        assert!(!slides.is_empty(), "should parse at least one slide");
        assert!(
            slides.len() >= 7,
            "slides.md has 8 slides separated by ---, got {}",
            slides.len()
        );

        let renderer = SlideRenderer::new(80, 24, settings, slides.len());
        for slide in &slides {
            let (lines, _) = renderer.render(slide);
            assert!(
                !lines.is_empty(),
                "slide {} should render lines",
                slide.index
            );
        }
    }

    #[test]
    fn parse_table_slide() {
        let md = r#"---
marp: true
---

## Navigation

| Key               | Action     |
| ----------------- | ---------- |
| `j` / `Space`     | Next slide |
| `k` / `Backspace` | Previous   |
| `←` `→`           | Arrow keys |
"#;
        let (slides, _) = parse_markdown(md);
        assert!(!slides.is_empty());
        let table_slide = &slides[0];
        let has_table = table_slide
            .content
            .iter()
            .any(|e| matches!(e, SlideElement::Table(_)));
        assert!(has_table, "should contain a Table element");
    }

    #[test]
    fn parse_inline_formatting() {
        let md = "---\nmarp: true\n---\n\n# Title\n\n**bold** and *italic* and ~~strike~~\n";
        let (slides, _) = parse_markdown(md);
        assert!(!slides.is_empty());
        let has_paragraph = slides[0]
            .content
            .iter()
            .any(|e| matches!(e, SlideElement::Paragraph(_)));
        assert!(
            has_paragraph,
            "should have a paragraph with inline formatting"
        );
    }

    #[test]
    fn parse_heading_divider_splits() {
        let md = r#"---
marp: true
headingDivider: 2
---

# First Slide

Some content

## Second Slide

More content

## Third Slide

Even more
"#;
        let (slides, _) = parse_markdown(md);
        assert!(
            slides.len() >= 3,
            "headingDivider: 2 should split at h2 headings, got {} slides",
            slides.len()
        );
    }

    #[test]
    fn parse_numbered_list() {
        let md = "---\nmarp: true\n---\n\n# Lists\n\n1. First\n2. Second\n3. Third\n";
        let (slides, _) = parse_markdown(md);
        assert!(!slides.is_empty());
        let has_numbered = slides[0]
            .content
            .iter()
            .any(|e| matches!(e, SlideElement::NumberedList(_, _)));
        assert!(has_numbered, "should contain a NumberedList element");
    }

    #[test]
    fn parse_paginate_setting() {
        let md = "---\nmarp: true\npaginate: true\n---\n\n# Hello\n";
        let (_, settings) = parse_markdown(md);
        assert!(settings.paginate, "paginate should be true");
    }

    #[test]
    fn parse_invert_class() {
        let md = "---\nmarp: true\nclass: invert\n---\n\n# Hello\n";
        let (slides, _) = parse_markdown(md);
        assert!(!slides.is_empty());
        assert!(
            slides[0].class.contains(&"invert".to_string()),
            "slide should have invert class"
        );
    }
}
