mod app;
mod brew;
mod indexer;
mod ollama;
mod log;
mod rag;
mod ui;
mod vector_store;

use anyhow::Result;
use app::{App, AppState};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ollama::OllamaClient;
use rag::RagPipeline;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use vector_store::VectorStore;

#[derive(Debug, Clone, Copy)]
enum AppCommand {
    Continue,
    Quit,
    Rebuild,
}

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

    // Initialize Ollama client
    let ollama = OllamaClient::new("llama3.2".to_string());

    // Initialize vector store
    let db_path = get_db_path()?;
    let mut vector_store = VectorStore::new(db_path)?;

    // Create app
    let mut app = App::new();

    // Index if needed
    if vector_store.is_empty()? {
        app.set_status(Some("Indexing man pages... This may take a few minutes.".to_string()));
        terminal.draw(|f| ui::render(f, &app))?;

        // Index packages
        let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();
        let docs = indexer::index_brew_packages(&package_names).await?;
        let total = docs.len();

        // Process each doc
        for (i, doc) in docs.into_iter().enumerate() {
            let cmd_name = doc.command_name.clone();
            match ollama.generate_embedding(&doc.man_content).await {
                Ok(embedding) => {
                    if let Err(e) = vector_store.store_command(
                        &doc.package_name,
                        &cmd_name,
                        &doc.man_content,
                        &embedding,
                    ) {
                        crate::log::log_error(&format!("Failed to store: {}: {}", cmd_name, e));
                    }
                }
                Err(e) => {
                    crate::log::log_error(&format!("Failed to embed: {}: {}", cmd_name, e));
                }
            }

            // Update status
            app.set_status(Some(format!("Indexed {}/{} commands", i + 1, total)));
            terminal.draw(|f| ui::render(f, &app))?;
        }

        let count = vector_store.count()?;
        app.set_status(Some(format!("Indexed {} commands. Ready!", count)));
        terminal.draw(|f| ui::render(f, &app))?;

        // Small delay so user sees completion message
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    app.set_status(None);
    app.clear_input();

    // Initialize RAG pipeline
    let rag = RagPipeline::new(&vector_store, &ollama);

    // Run the app loop
    loop {
        let cmd = {
            let rag = RagPipeline::new(&vector_store, &ollama);
            run_app(&mut terminal, &mut app, &rag, &packages).await?
        };

        match cmd {
            AppCommand::Quit => break,
            AppCommand::Rebuild => {
                // Rebuild knowledge base
                if let Err(e) = rebuild_knowledge_base(&mut vector_store, &ollama, &packages, &mut terminal, &mut app).await {
                    app.set_response(format!("Error rebuilding: {}", e));
                }
                app.clear_input();
            }
            AppCommand::Continue => {}
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

async fn run_app<'a>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rag: &RagPipeline<'a>,
    packages: &[brew::BrewPackage],
) -> Result<AppCommand> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') => {
                    return Ok(AppCommand::Quit);
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    return Ok(AppCommand::Rebuild);
                }
                KeyCode::Up => {
                    app.scroll_up();
                }
                KeyCode::Down => {
                    app.scroll_down();
                }
                KeyCode::Char(c) if matches!(app.state, AppState::Input) => {
                    app.push_char(c);
                }
                KeyCode::Backspace if matches!(app.state, AppState::Input) => {
                    app.pop_char();
                }
                KeyCode::Enter if matches!(app.state, AppState::Input) => {
                    let query = app.input.clone();
                    if !query.is_empty() {
                        app.set_loading();
                        terminal.draw(|f| ui::render(f, app))?;

                        let package_names: Vec<String> =
                            packages.iter().map(|p| p.name.clone()).collect();

                        match rag.query_with_rag(&query, &package_names, 2).await {
                            Ok(response) => {
                                app.set_response(response);
                                app.clear_input();
                            }
                            Err(e) => {
                                app.set_response(format!("Error: {}", e));
                            }
                        }

                        // Return to input mode for next query
                        app.state = AppState::Input;
                    }
                }
                KeyCode::Esc if matches!(app.state, AppState::Input) => {
                    if !app.input.is_empty() {
                        app.clear_input();
                    }
                }
                _ => {}
            }
        }

        if app.should_quit {
            return Ok(AppCommand::Quit);
        }
    }
}

fn get_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let app_dir = home.join(".mac-aid");
    std::fs::create_dir_all(&app_dir)?;
    Ok(app_dir.join("commands.db"))
}

async fn rebuild_knowledge_base(
    vector_store: &mut VectorStore,
    ollama: &OllamaClient,
    packages: &[brew::BrewPackage],
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    // Clear existing entries before rebuilding
    vector_store.clear()?;
    app.set_status(Some("Rebuilding knowledge base...".to_string()));
    terminal.draw(|f| ui::render(f, app))?;

    // Index packages
    let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();
    let docs = indexer::index_brew_packages(&package_names).await?;
    let total = docs.len();

    // Process each doc
        for (i, doc) in docs.into_iter().enumerate() {
        let cmd_name = doc.command_name.clone();
        match ollama.generate_embedding(&doc.man_content).await {
            Ok(embedding) => {
                if let Err(e) = vector_store.store_command(
                    &doc.package_name,
                    &cmd_name,
                    &doc.man_content,
                    &embedding,
                ) {
                    crate::log::log_error(&format!("Failed to store: {}: {}", cmd_name, e));
                }
            }
            Err(e) => {
                crate::log::log_error(&format!("Failed to embed: {}: {}", cmd_name, e));
            }
        }

        // Update status
        app.set_status(Some(format!("Rebuilding: {}/{} commands", i + 1, total)));
        terminal.draw(|f| ui::render(f, app))?;
    }

    let count = vector_store.count()?;
    app.set_status(Some(format!("Knowledge base rebuilt! {} commands indexed.", count)));
    terminal.draw(|f| ui::render(f, app))?;

    // Small delay so user sees completion message
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    app.set_status(None);

    Ok(())
}
