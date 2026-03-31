use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute, terminal,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};
use std::sync::Mutex;

mod parser;
mod renderer;

use parser::{DeckSettings, Slide, SlideElement};
use renderer::{ImageRegion, SlideRenderer};

#[derive(Parser)]
#[command(name = "tui-deck")]
#[command(about = "Terminal slide deck presenter", long_about = None)]
struct Args {
    #[arg(default_value = "slides.md")]
    file: PathBuf,

    #[arg(long)]
    presenter: bool,

    #[arg(long, default_value = "/tmp/tui-deck.sock")]
    socket: PathBuf,
}

struct AppState {
    slides: Vec<Slide>,
    current_index: usize,
    start_time: Instant,
    settings: DeckSettings,
    picker: Picker,
    image_states: HashMap<String, StatefulProtocol>,
    slide_dir: PathBuf,
}

impl AppState {
    fn new(slides: Vec<Slide>, settings: DeckSettings, picker: Picker, slide_dir: PathBuf) -> Self {
        Self {
            slides,
            current_index: 0,
            start_time: Instant::now(),
            settings,
            picker,
            image_states: HashMap::new(),
            slide_dir,
        }
    }

    fn current_slide(&self) -> &Slide {
        &self.slides[self.current_index]
    }

    fn total_slides(&self) -> usize {
        self.slides.len()
    }

    fn elapsed_time(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    fn next(&mut self) {
        if self.current_index + 1 < self.slides.len() {
            self.current_index += 1;
            self.prune_image_states_for_current_slide();
        }
    }

    fn prev(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
            self.prune_image_states_for_current_slide();
        }
    }

    fn ensure_images_loaded(&mut self, regions: &[ImageRegion]) {
        for region in regions {
            let _ = self.ensure_image_loaded(region);
        }
    }

    fn ensure_image_loaded(&mut self, region: &ImageRegion) -> Result<()> {
        if self.image_states.contains_key(&region.url) {
            return Ok(());
        }

        let image_path = resolve_image_path(&self.slide_dir, &region.url);
        let dyn_img = image::ImageReader::open(&image_path)?.decode()?;
        let protocol = self.picker.new_resize_protocol(dyn_img);

        self.image_states.insert(region.url.clone(), protocol);
        Ok(())
    }

    fn prune_image_states_for_current_slide(&mut self) {
        let mut keep = HashSet::new();
        if let Some(bg) = self.current_slide().image.as_ref() {
            keep.insert(bg.url.clone());
        }
        for element in &self.current_slide().content {
            if let SlideElement::Image(img) = element {
                keep.insert(img.url.clone());
            }
        }

        self.image_states.retain(|url, _| keep.contains(url));
    }
}

fn render_slide(frame: &mut Frame, state: &mut AppState) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(frame.area());

    let slide = state.current_slide().clone();
    let renderer = SlideRenderer::new(
        chunks[0].width as usize,
        chunks[0].height as usize,
        state.settings.clone(),
        state.total_slides(),
    );
    let (lines, image_regions) = renderer.render(&slide);
    state.ensure_images_loaded(&image_regions);

    let has_bg_image = image_regions.iter().any(|region| region.is_background);
    let bg_color = slide
        .background_color
        .as_deref()
        .and_then(parse_css_color_simple)
        .unwrap_or(Color::Rgb(26, 26, 46));

    let inner_area = inset_block_area(chunks[0]);
    for region in image_regions.iter().filter(|r| r.is_background) {
        if let Some(image_rect) = image_region_rect(region, inner_area) {
            if let Some(protocol) = state.image_states.get_mut(&region.url) {
                frame.render_stateful_widget(StatefulImage::default(), image_rect, protocol);
            }
        }
    }

    let slide_paragraph = Paragraph::new(
        lines
            .iter()
            .map(|l| {
                Line::from(
                    l.spans
                        .iter()
                        .map(|s| ratatui::text::Span::styled(s.content.clone(), s.style))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
    .block(Block::default().title(" tui-deck ").borders(Borders::ALL))
    .style(if has_bg_image {
        Style::default()
    } else {
        Style::default().bg(bg_color)
    });

    frame.render_widget(slide_paragraph, chunks[0]);

    for region in image_regions.iter().filter(|r| !r.is_background) {
        if let Some(image_rect) = image_region_rect(region, inner_area) {
            if let Some(protocol) = state.image_states.get_mut(&region.url) {
                frame.render_stateful_widget(StatefulImage::default(), image_rect, protocol);
            }
        }
    }

    let progress_text = format!(
        " Slide {}/{} │ {} │ j/k or ←/→ Navigate │ q Quit ",
        state.current_index + 1,
        state.total_slides(),
        state.elapsed_time()
    );

    let progress =
        Paragraph::new(progress_text).style(Style::default().fg(Color::Rgb(100, 100, 100)));
    frame.render_widget(progress, chunks[1]);
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PresenterMessage {
    current_slide: usize,
    total_slides: usize,
    notes: Option<String>,
    title: Option<String>,
}

fn create_message(slide: &Slide, total: usize) -> PresenterMessage {
    PresenterMessage {
        current_slide: slide.index,
        total_slides: total,
        notes: slide.notes.clone(),
        title: slide.title.clone(),
    }
}

struct ClientState {
    current_index: usize,
    total_slides: usize,
    notes: Option<String>,
    title: Option<String>,
    start_time: Instant,
    connected: bool,
}

impl ClientState {
    fn new() -> Self {
        Self {
            current_index: 0,
            total_slides: 0,
            notes: None,
            title: None,
            start_time: Instant::now(),
            connected: false,
        }
    }

    fn elapsed_time(&self) -> String {
        let elapsed = self.start_time.elapsed();
        format!(
            "{:02}:{:02}",
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60
        )
    }
}

fn run_presenter_client(
    socket_path: PathBuf,
    slides: Vec<Slide>,
    settings: DeckSettings,
    slide_dir: PathBuf,
) -> Result<()> {
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let mut image_states: HashMap<String, StatefulProtocol> = HashMap::new();

    terminal::enable_raw_mode()?;
    execute!(std::io::stdout(), terminal::EnterAlternateScreen)?;

    let client_state = Arc::new(Mutex::new(ClientState::new()));
    let client_state_clone = client_state.clone();

    std::thread::spawn(
        move || match std::os::unix::net::UnixStream::connect(&socket_path) {
            Ok(mut stream) => loop {
                let mut buf = [0u8; 4096];
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(msg) = serde_json::from_slice::<PresenterMessage>(&buf[..n]) {
                            if let Ok(mut state) = client_state_clone.lock() {
                                state.current_index = msg.current_slide;
                                state.total_slides = msg.total_slides;
                                state.notes = msg.notes;
                                state.title = msg.title;
                                state.connected = true;
                            }
                        }
                    }
                    Err(_) => break,
                }
            },
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        },
    );

    let mut quit = false;

    while !quit {
        let state = {
            match client_state.lock() {
                Ok(s) => (
                    s.current_index,
                    s.total_slides,
                    s.notes.clone(),
                    s.elapsed_time(),
                    s.connected,
                ),
                Err(_) => continue,
            }
        };

        terminal
            .draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(50),
                        Constraint::Percentage(25),
                        Constraint::Percentage(25),
                    ])
                    .split(f.area());

                if !state.4 {
                    let connecting = Paragraph::new("Connecting to presentation...")
                        .style(Style::default().fg(Color::Yellow));
                    f.render_widget(connecting, chunks[0]);
                    return;
                }

                let slide = &slides[state.0.min(slides.len().saturating_sub(1))];
                let renderer = SlideRenderer::new(
                    chunks[0].width as usize,
                    chunks[0].height as usize,
                    settings.clone(),
                    slides.len(),
                );
                let (current_lines, current_regions) = renderer.render(slide);
                ensure_protocols_loaded(
                    &mut picker,
                    &mut image_states,
                    &slide_dir,
                    &current_regions,
                );

                let current_inner = inset_block_area(chunks[0]);
                for region in current_regions.iter().filter(|r| r.is_background) {
                    if let Some(image_rect) = image_region_rect(region, current_inner) {
                        if let Some(protocol) = image_states.get_mut(&region.url) {
                            f.render_stateful_widget(
                                StatefulImage::default(),
                                image_rect,
                                protocol,
                            );
                        }
                    }
                }

                let current = Paragraph::new(
                    current_lines
                        .iter()
                        .map(|l| {
                            Line::from(
                                l.spans
                                    .iter()
                                    .map(|s| {
                                        ratatui::text::Span::styled(s.content.clone(), s.style)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .block(
                    Block::default()
                        .title(" Current Slide ")
                        .borders(Borders::ALL),
                )
                .style(Style::default().bg(Color::Rgb(26, 26, 46)));
                f.render_widget(current, chunks[0]);

                for region in current_regions.iter().filter(|r| !r.is_background) {
                    if let Some(image_rect) = image_region_rect(region, current_inner) {
                        if let Some(protocol) = image_states.get_mut(&region.url) {
                            f.render_stateful_widget(
                                StatefulImage::default(),
                                image_rect,
                                protocol,
                            );
                        }
                    }
                }

                if state.0 + 1 < slides.len() {
                    let next = &slides[state.0 + 1];
                    let next_renderer = SlideRenderer::new(
                        chunks[1].width as usize,
                        chunks[1].height as usize,
                        settings.clone(),
                        slides.len(),
                    );
                    let (next_lines, next_regions) = next_renderer.render(next);
                    ensure_protocols_loaded(
                        &mut picker,
                        &mut image_states,
                        &slide_dir,
                        &next_regions,
                    );

                    let next_inner = inset_block_area(chunks[1]);
                    for region in next_regions.iter().filter(|r| r.is_background) {
                        if let Some(image_rect) = image_region_rect(region, next_inner) {
                            if let Some(protocol) = image_states.get_mut(&region.url) {
                                f.render_stateful_widget(
                                    StatefulImage::default(),
                                    image_rect,
                                    protocol,
                                );
                            }
                        }
                    }

                    let next_paragraph = Paragraph::new(
                        next_lines
                            .iter()
                            .map(|l| {
                                Line::from(
                                    l.spans
                                        .iter()
                                        .map(|s| {
                                            ratatui::text::Span::styled(
                                                s.content.clone(),
                                                s.style.fg(Color::Rgb(128, 128, 128)),
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                    .block(Block::default().title(" Next Slide ").borders(Borders::ALL))
                    .style(Style::default().bg(Color::Rgb(26, 26, 46)));

                    f.render_widget(next_paragraph, chunks[1]);

                    for region in next_regions.iter().filter(|r| !r.is_background) {
                        if let Some(image_rect) = image_region_rect(region, next_inner) {
                            if let Some(protocol) = image_states.get_mut(&region.url) {
                                f.render_stateful_widget(
                                    StatefulImage::default(),
                                    image_rect,
                                    protocol,
                                );
                            }
                        }
                    }
                }

                let notes_text = state.2.clone().unwrap_or_else(|| "No notes".to_string());
                let notes = Paragraph::new(notes_text)
                    .block(
                        Block::default()
                            .title(format!(" Notes (Slide {}/{}) ", state.0 + 1, state.1))
                            .borders(Borders::ALL),
                    )
                    .style(Style::default().fg(Color::White));

                f.render_widget(notes, chunks[2]);
            })
            .unwrap();

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if let KeyCode::Char('q') | KeyCode::Char('Q') = key.code {
                        quit = true;
                    }
                }
            }
        }
    }

    terminal::disable_raw_mode()?;
    execute!(std::io::stdout(), terminal::LeaveAlternateScreen)?;

    Ok(())
}

fn run_server(
    socket_path: PathBuf,
    slides: Vec<Slide>,
    settings: DeckSettings,
    slide_dir: PathBuf,
) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    let state = Arc::new(Mutex::new(AppState::new(
        slides, settings, picker, slide_dir,
    )));

    terminal::enable_raw_mode()?;
    execute!(std::io::stdout(), terminal::EnterAlternateScreen)?;

    let state_clone = state.clone();

    std::thread::spawn(move || {
        let mut clients: Vec<std::os::unix::net::UnixStream> = Vec::new();

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(true);
                    clients.push(stream);
                }
                Err(_) => {}
            }

            let current_state = {
                match state_clone.lock() {
                    Ok(s) => (s.current_slide().clone(), s.total_slides()),
                    Err(_) => continue,
                }
            };

            let msg = serde_json::to_vec(&create_message(&current_state.0, current_state.1))
                .unwrap_or_default();

            clients.retain(|mut client| match client.write_all(&msg) {
                Ok(_) => true,
                Err(_) => false,
            });

            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    loop {
        let should_quit = {
            let mut s = state.lock().unwrap();

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => true,
                            KeyCode::Char('j')
                            | KeyCode::Char('J')
                            | KeyCode::Char(' ')
                            | KeyCode::Right => {
                                s.next();
                                false
                            }
                            KeyCode::Char('k')
                            | KeyCode::Char('K')
                            | KeyCode::Backspace
                            | KeyCode::Left => {
                                s.prev();
                                false
                            }
                            KeyCode::Char('l') | KeyCode::Char('L') => {
                                s.next();
                                false
                            }
                            KeyCode::Char('h') | KeyCode::Char('H') => {
                                s.prev();
                                false
                            }
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        };

        terminal
            .draw(|f| {
                let mut s = state.lock().unwrap();
                render_slide(f, &mut s);
            })
            .unwrap();

        if should_quit {
            break;
        }
    }

    let _ = std::fs::remove_file(&socket_path);

    terminal::disable_raw_mode()?;
    execute!(std::io::stdout(), terminal::LeaveAlternateScreen)?;

    Ok(())
}

fn resolve_image_path(slide_dir: &Path, url: &str) -> PathBuf {
    let raw = Path::new(url);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        slide_dir.join(raw)
    }
}

fn ensure_protocols_loaded(
    picker: &mut Picker,
    image_states: &mut HashMap<String, StatefulProtocol>,
    slide_dir: &Path,
    regions: &[ImageRegion],
) {
    for region in regions {
        if image_states.contains_key(&region.url) {
            continue;
        }

        let path = resolve_image_path(slide_dir, &region.url);
        let Ok(reader) = image::ImageReader::open(path) else {
            continue;
        };
        let Ok(img) = reader.decode() else {
            continue;
        };

        let protocol = picker.new_resize_protocol(img);
        image_states.insert(region.url.clone(), protocol);
    }
}

fn inset_block_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn parse_region_lines(value: &str, total: u16, fallback: u16) -> u16 {
    let trimmed = value.trim();
    let number = trimmed
        .trim_end_matches("px")
        .trim_end_matches('%')
        .trim()
        .parse::<u16>()
        .ok();

    let Some(value) = number else {
        return fallback;
    };

    if trimmed.ends_with('%') {
        ((total.saturating_mul(value)).saturating_div(100)).max(1)
    } else {
        value.saturating_div(24).max(1)
    }
}

fn image_region_rect(region: &ImageRegion, inner_area: Rect) -> Option<Rect> {
    if inner_area.width == 0 || inner_area.height == 0 {
        return None;
    }

    let line = u16::try_from(region.line_index).ok()?;
    if line >= inner_area.height {
        return None;
    }

    let y = inner_area.y.saturating_add(line);
    let available_h = inner_area.height.saturating_sub(line);
    let fallback_h = u16::try_from(region.height_lines)
        .unwrap_or(available_h)
        .max(1);
    let mut h = region
        .height_spec
        .as_deref()
        .map(|v| parse_region_lines(v, inner_area.height, fallback_h))
        .unwrap_or(fallback_h)
        .min(available_h)
        .max(1);

    if region.is_background {
        h = inner_area.height;
    }

    let mut w = region
        .width
        .as_deref()
        .map(|v| parse_region_lines(v, inner_area.width, inner_area.width))
        .unwrap_or(inner_area.width)
        .min(inner_area.width)
        .max(1);

    let mut x = inner_area.x;
    if region.is_background {
        w = inner_area.width;
        h = inner_area.height;
    } else if w < inner_area.width {
        x = inner_area.x + (inner_area.width - w) / 2;
    }

    Some(Rect {
        x,
        y,
        width: w,
        height: h,
    })
}

fn parse_css_color_simple(color: &str) -> Option<Color> {
    let trimmed = color.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    None
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut file = std::fs::File::open(&args.file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let (slides, settings) = parser::parse_markdown(&content);

    if slides.is_empty() {
        anyhow::bail!("No slides found in {}", args.file.display());
    }

    let slide_dir = args
        .file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    if args.presenter {
        run_presenter_client(args.socket, slides, settings, slide_dir)
    } else {
        run_server(args.socket, slides, settings, slide_dir)
    }
}
