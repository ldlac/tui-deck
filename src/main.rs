use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
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
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::sync::Mutex;

mod parser;
mod renderer;

use parser::{DeckSettings, Slide};
use renderer::SlideRenderer;

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
}

impl AppState {
    fn new(slides: Vec<Slide>, settings: DeckSettings) -> Self {
        Self {
            slides,
            current_index: 0,
            start_time: Instant::now(),
            settings,
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
        }
    }

    fn prev(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
        }
    }
}

fn render_slide(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(frame.area());

    let slide = state.current_slide();
    let renderer = SlideRenderer::new(
        chunks[0].width as usize,
        chunks[0].height as usize,
        state.settings.clone(),
        state.total_slides(),
    );
    let lines = renderer.render(slide);

    let bg_color = slide
        .background_color
        .as_deref()
        .and_then(|c| parse_css_color_simple(c))
        .unwrap_or(Color::Rgb(26, 26, 46));

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
    .style(Style::default().bg(bg_color));

    frame.render_widget(slide_paragraph, chunks[0]);

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
) -> Result<()> {
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

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
                let current_lines = renderer.render(slide);

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

                if state.0 + 1 < slides.len() {
                    let next = &slides[state.0 + 1];
                    let next_renderer = SlideRenderer::new(
                        chunks[1].width as usize,
                        chunks[1].height as usize,
                        settings.clone(),
                        slides.len(),
                    );
                    let next_lines = next_renderer.render(next);

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

fn run_server(socket_path: PathBuf, slides: Vec<Slide>, settings: DeckSettings) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    let state = Arc::new(Mutex::new(AppState::new(slides, settings)));

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

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
                let s = state.lock().unwrap();
                render_slide(f, &s);
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

    if args.presenter {
        run_presenter_client(args.socket, slides, settings)
    } else {
        run_server(args.socket, slides, settings)
    }
}
