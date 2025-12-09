mod app;
mod brew;
mod ollama;
mod ui;

use anyhow::Result;
use app::{App, AppState};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ollama::OllamaClient;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load Homebrew packages
    let packages = brew::get_installed_packages()?;
    let mut app = App::new(packages);

    // Initialize Ollama client
    let ollama = OllamaClient::new("llama3.2".to_string());

    // Run the app
    let res = run_app(&mut terminal, &mut app, &ollama).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ollama: &OllamaClient,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match app.state {
                AppState::Normal => match key.code {
                    KeyCode::Char('q') => {
                        app.quit();
                    }
                    KeyCode::Char('i') => {
                        app.enter_input_mode();
                    }
                    KeyCode::Up => {
                        app.previous();
                    }
                    KeyCode::Down => {
                        app.next();
                    }
                    _ => {}
                },
                AppState::Input => match key.code {
                    KeyCode::Enter => {
                        let query = app.input.clone();
                        if !query.is_empty() {
                            app.set_loading();
                            terminal.draw(|f| ui::render(f, app))?;

                            let package_names: Vec<String> =
                                app.packages.iter().map(|p| p.name.clone()).collect();

                            match ollama.query(&query, &package_names).await {
                                Ok(response) => {
                                    app.set_response(response);
                                }
                                Err(e) => {
                                    app.set_response(format!("Error: {}", e));
                                }
                            }
                        } else {
                            app.exit_input_mode();
                        }
                    }
                    KeyCode::Char(c) => {
                        app.push_char(c);
                    }
                    KeyCode::Backspace => {
                        app.pop_char();
                    }
                    KeyCode::Esc => {
                        app.exit_input_mode();
                    }
                    _ => {}
                },
                AppState::Loading => {
                    // Ignore input while loading
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
