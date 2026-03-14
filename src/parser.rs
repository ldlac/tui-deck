use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub index: usize,
    pub title: Option<String>,
    pub content: Vec<SlideElement>,
    pub notes: Option<String>,
    pub slide_type: SlideType,
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
pub enum SlideElement {
    Heading(usize, String), // level, text
    Paragraph(String),
    CodeBlock(String, String), // language, code
    BulletList(Vec<String>),
    NumberedList(Vec<String>),
    Blockquote(String),
    HorizontalRule,
    Plain(String),
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
        }
    }

    pub fn parse(&mut self, markdown: &str) -> Vec<Slide> {
        let mut parser = pulldown_cmark::Parser::new(markdown);
        let events: Vec<Event> = parser.collect();

        for event in events {
            self.process_event(event);
        }

        // Flush remaining slide
        self.finish_current_slide();

        // If no slides created, create a default one
        if self.slides.is_empty() {
            self.slides.push(Slide {
                index: 0,
                title: None,
                content: vec![SlideElement::Plain("No content".to_string())],
                notes: None,
                slide_type: SlideType::Content,
            });
        }

        // Re-index slides
        for (i, slide) in self.slides.iter_mut().enumerate() {
            slide.index = i;
        }

        self.slides.clone()
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
                });
                // Capture title from subsequent text
            }
            Event::End(TagEnd::Heading(_)) => {
                // Title already captured in text
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush_list();
                self.in_code_block = true;
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let is_ascii = lang == "ascii" || lang == "art";
                self.in_ascii_block = is_ascii;
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
                        });
                    }
                }
                self.in_code_block = false;
                self.in_ascii_block = false;
            }
            Event::Text(text) => {
                if self.in_code_block {
                    if let Some((ref mut lang, ref mut code)) = self.code_buffer {
                        if !code.is_empty() {
                            code.push('\n');
                        }
                        code.push_str(&text);
                    }
                } else if self.in_ascii_block {
                    if let Some((ref mut lang, ref mut code)) = self.code_buffer {
                        if !code.is_empty() {
                            code.push('\n');
                        }
                        code.push_str(&text);
                    }
                } else if let Some(ref mut list) = self.current_list {
                    list.push(text.to_string());
                } else if let Some(ref mut slide) = self.current_slide {
                    // First text in title slide becomes title
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
            Event::End(TagEnd::Paragraph) => {
                // Handled with text
            }
            Event::Start(Tag::List(_)) => {
                self.flush_list();
                self.current_list = Some(Vec::new());
                self.list_type = Some(ListType::Bullet);
            }
            Event::End(TagEnd::List(_)) => {
                self.flush_list();
            }
            Event::End(TagEnd::Item) => {
                // List item complete, will flush on next item or list end
            }
            Event::Rule => {
                self.finish_current_slide();
            }
            Event::Html(html) => {
                // Check for notes: <!-- notes: ... -->
                let html_str = html.to_string();
                if let Some(notes) = extract_notes(&html_str) {
                    if let Some(ref mut slide) = self.current_slide {
                        slide.notes = Some(notes);
                    }
                }
            }
            Event::Code(code) => {
                if let Some(ref mut list) = self.current_list {
                    list.push(format!("`{}`", code));
                } else {
                    self.current_elements
                        .push(SlideElement::Plain(format!("`{}`", code)));
                }
            }
            _ => {}
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

        if let Some(mut slide) = self.current_slide.take() {
            if !self.current_elements.is_empty() || slide.title.is_some() {
                slide.content = std::mem::take(&mut self.current_elements);

                // Determine slide type if not already set
                if slide.slide_type == SlideType::Content {
                    slide.slide_type = determine_slide_type(&slide);
                }

                self.slides.push(slide);
            }
            self.current_elements = Vec::new();
        }
    }
}

fn extract_notes(html: &str) -> Option<String> {
    // Match <!-- notes: ... --> or <!--notes:...-->
    let trimmed = html.trim();
    if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
        let inner = &trimmed[4..trimmed.len() - 3];
        if let Some(notes) = inner.strip_prefix("notes:") {
            return Some(notes.trim().to_string());
        }
        if let Some(notes) = inner.strip_prefix("notes ") {
            return Some(notes.trim().to_string());
        }
    }
    None
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
        // Check if any code block is ascii art
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

pub fn parse_markdown(markdown: &str) -> Vec<Slide> {
    let mut parser = MarkdownParser::new();
    parser.parse(markdown)
}
