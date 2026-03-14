use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub index: usize,
    pub title: Option<String>,
    pub content: Vec<SlideElement>,
    pub notes: Option<String>,
    pub slide_type: SlideType,
    pub class: Vec<String>,
    pub background_color: Option<String>,
    pub image: Option<ImageSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSpec {
    pub url: String,
    pub width: Option<String>,
    pub height: Option<String>,
    pub position: Option<String>,
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
    pub size: Option<String>,
}

impl Default for DeckSettings {
    fn default() -> Self {
        Self {
            theme: None,
            paginate: false,
            class: Vec::new(),
            background_color: None,
            size: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlideElement {
    Heading(usize, String),
    Paragraph(String),
    CodeBlock(String, String),
    BulletList(Vec<String>),
    NumberedList(Vec<String>),
    Blockquote(String),
    HorizontalRule,
    Plain(String),
    Image(ImageSpec),
    ColumnBreak,
    Table(Vec<Vec<String>>),
}

pub struct MarkdownParser {
    slides: Vec<Slide>,
    current_slide: Option<Slide>,
    current_elements: Vec<SlideElement>,
    current_list: Option<Vec<String>>,
    list_type: Option<ListType>,
    code_buffer: Option<(String, String)>,
    in_code_block: bool,
    in_ascii_block: bool,
    in_presenter_notes: bool,
    notes_buffer: String,
    settings: DeckSettings,
    pending_class: Option<Vec<String>>,
    in_table: bool,
    table_headers: Vec<String>,
    current_table_row: Vec<String>,
    table_rows: Vec<Vec<String>>,
    in_table_cell: bool,
    current_cell_content: String,
}

#[derive(Clone)]
enum ListType {
    Bullet,
    Numbered(u32),
}

impl MarkdownParser {
    pub fn new() -> Self {
        Self {
            slides: Vec::new(),
            current_slide: None,
            current_elements: Vec::new(),
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
            table_headers: Vec::new(),
            current_table_row: Vec::new(),
            table_rows: Vec::new(),
            in_table_cell: false,
            current_cell_content: String::new(),
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
                content: vec![SlideElement::Plain("No content".to_string())],
                notes: None,
                slide_type: SlideType::Content,
                class: Vec::new(),
                background_color: None,
                image: None,
            });
        }

        for (i, slide) in self.slides.iter_mut().enumerate() {
            slide.index = i;
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
                let value = value.trim();
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
                    "size" => self.settings.size = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }

    fn process_event(&mut self, event: Event) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush_list();
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.current_slide = Some(Slide {
                    index: 0,
                    title: None,
                    content: Vec::new(),
                    notes: None,
                    slide_type: if level == 1 {
                        SlideType::Title
                    } else {
                        SlideType::Content
                    },
                    class: self.pending_class.take().unwrap_or_default(),
                    background_color: None,
                    image: None,
                });
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
                            content: Vec::new(),
                            notes: None,
                            slide_type,
                            class: self.pending_class.take().unwrap_or_default(),
                            background_color: None,
                            image: None,
                        });
                    }
                }
                self.in_code_block = false;
                self.in_ascii_block = false;
            }
            Event::Text(text) => {
                let trimmed = text.trim();
                if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                    self.finish_current_slide();
                } else if self.in_presenter_notes {
                    if !self.notes_buffer.is_empty() {
                        self.notes_buffer.push('\n');
                    }
                    self.notes_buffer.push_str(&text);
                } else if self.in_code_block || self.in_ascii_block {
                    if let Some((_, ref mut code)) = self.code_buffer {
                        if !code.is_empty() {
                            code.push('\n');
                        }
                        code.push_str(&text);
                    }
                } else if self.in_table_cell {
                    if !self.current_cell_content.is_empty() {
                        self.current_cell_content.push(' ');
                    }
                    self.current_cell_content.push_str(&text);
                } else if self.in_table {
                    self.current_table_row.push(text.to_string());
                } else if let Some(ref mut list) = self.current_list {
                    list.push(text.to_string());
                } else if let Some(ref mut slide) = self.current_slide {
                    if slide.title.is_none() && !text.trim().is_empty() {
                        slide.title = Some(text.trim().to_string());
                    }
                } else if !text.trim().is_empty() {
                    self.current_elements
                        .push(SlideElement::Plain(text.to_string()));
                }
            }
            Event::Start(Tag::Paragraph) => {
                self.flush_list();
            }
            Event::End(TagEnd::Paragraph) => {}
            Event::Start(Tag::List(_)) => {
                self.flush_list();
                self.current_list = Some(Vec::new());
                self.list_type = Some(ListType::Bullet);
            }
            Event::End(TagEnd::List(_)) => {
                self.flush_list();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.flush_list();
            }
            Event::End(TagEnd::BlockQuote(_)) => {}
            Event::Start(Tag::Item) => {
                if self.current_list.is_none() {
                    self.current_list = Some(Vec::new());
                }
            }
            Event::End(TagEnd::Item) => {}
            Event::Start(Tag::Table(_)) => {
                self.flush_list();
                self.in_table = true;
                self.table_headers.clear();
                self.current_table_row.clear();
                self.table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                if !self.table_headers.is_empty() {
                    self.table_rows.insert(0, self.table_headers.clone());
                }
                if !self.table_rows.is_empty() {
                    self.current_elements
                        .push(SlideElement::Table(std::mem::take(&mut self.table_rows)));
                }
                self.in_table = false;
                self.table_headers.clear();
                self.current_table_row.clear();
            }
            Event::Start(Tag::TableHead) => {
                self.current_table_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                self.table_headers = self.current_table_row.clone();
            }
            Event::Start(Tag::TableRow) => {
                self.current_table_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if self.in_table {
                    let is_separator = self
                        .current_table_row
                        .iter()
                        .all(|c| c.chars().all(|ch| ch == '-' || ch == ' ' || ch == ':'));
                    if !is_separator && !self.current_table_row.is_empty() {
                        self.table_rows
                            .push(std::mem::take(&mut self.current_table_row));
                        self.current_table_row = Vec::new();
                    }
                }
            }
            Event::Start(Tag::TableCell) => {
                self.in_table_cell = true;
                self.current_cell_content.clear();
            }
            Event::End(TagEnd::TableCell) => {
                if self.in_table_cell {
                    self.current_table_row
                        .push(std::mem::take(&mut self.current_cell_content));
                    self.in_table_cell = false;
                }
            }
            Event::Rule => {
                self.finish_current_slide();
            }
            Event::Html(html) => {
                self.process_html_comment(&html.to_string());
            }
            Event::Code(code) => {
                if self.in_table_cell {
                    if !self.current_cell_content.is_empty() {
                        self.current_cell_content.push(' ');
                    }
                    self.current_cell_content.push_str(&format!("`{}`", code));
                } else if let Some(ref mut list) = self.current_list {
                    list.push(format!("`{}`", code));
                } else {
                    self.current_elements
                        .push(SlideElement::Plain(format!("`{}`", code)));
                }
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

        if inner.starts_with("notes:") {
            if let Some(notes) = inner.strip_prefix("notes:") {
                if let Some(ref mut slide) = self.current_slide {
                    slide.notes = Some(notes.trim().to_string());
                }
            }
            return;
        }

        if inner.starts_with("?") {
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

        if inner.trim() == "paginate: true" {
            self.settings.paginate = true;
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
                    Some(ListType::Bullet) => SlideElement::BulletList(list),
                    Some(ListType::Numbered(_)) => SlideElement::NumberedList(list),
                    None => SlideElement::BulletList(list),
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

pub fn parse_markdown(markdown: &str) -> (Vec<Slide>, DeckSettings) {
    let mut parser = MarkdownParser::new();
    parser.parse(markdown)
}
