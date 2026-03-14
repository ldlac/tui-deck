use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute, terminal,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
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

use parser::Slide;
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
    is_presenter_mode: bool,
}

impl AppState {
    fn new(slides: Vec<Slide>, is_presenter: bool) -> Self {
        Self {
            slides,
            current_index: 0,
            start_time: Instant::now(),
            is_presenter_mode: is_presenter,
        }
    }

    fn current_slide(&self) -> &Slide {
        &self.slides[self.current_index]
    }

    fn next_slide(&self) -> Option<&Slide> {
        if self.current_index + 1 < self.slides.len() {
            Some(&self.slides[self.current_index + 1])
        } else {
            None
        }
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

fn render_app(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(frame.area());

    let slide = state.current_slide();
    let renderer = SlideRenderer::new(chunks[0].width as usize, chunks[0].height as usize);
    let lines = renderer.render(slide);

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
    .style(Style::default().bg(Color::Rgb(26, 26, 46)));

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

fn render_presenter(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(frame.area());

    let current_slide = state.current_slide();
    let next_slide = state.next_slide();

    let renderer = SlideRenderer::new(chunks[0].width as usize, chunks[0].height as usize);
    let current_lines = renderer.render(current_slide);

    let current = Paragraph::new(
        current_lines
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
    .block(
        Block::default()
            .title(" Current Slide ")
            .borders(Borders::ALL),
    )
    .style(Style::default().bg(Color::Rgb(26, 26, 46)));

    frame.render_widget(current, chunks[0]);

    if let Some(next) = next_slide {
        let next_renderer = SlideRenderer::new(chunks[1].width as usize, chunks[1].height as usize);
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

        frame.render_widget(next_paragraph, chunks[1]);
    }

    let notes_text = current_slide
        .notes
        .clone()
        .unwrap_or_else(|| "No notes".to_string());

    let notes = Paragraph::new(notes_text)
        .block(
            Block::default()
                .title(format!(
                    " Notes (Slide {}/{}) ",
                    state.current_index + 1,
                    state.total_slides()
                ))
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(notes, chunks[2]);
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut file = std::fs::File::open(&args.file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let slides = parser::parse_markdown(&content);

    if slides.is_empty() {
        anyhow::bail!("No slides found in {}", args.file.display());
    }

    let state = Arc::new(Mutex::new(AppState::new(slides, args.presenter)));

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    terminal::enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;

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

        let current_state = {
            let s = state.lock().unwrap();
            (
                s.current_index,
                s.total_slides(),
                s.is_presenter_mode,
                s.current_slide().clone(),
            )
        };

        terminal
            .draw(|f| {
                let mut state_guard = AppState {
                    slides: vec![current_state.3],
                    current_index: 0,
                    start_time: Instant::now(),
                    is_presenter_mode: current_state.2,
                };

                if current_state.2 {
                    render_presenter(f, &mut state_guard);
                } else {
                    render_app(f, &mut state_guard);
                }
            })
            .unwrap();

        if should_quit {
            break;
        }
    }

    terminal::disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;

    Ok(())
}
