mod app;
mod brew;
mod indexer;
mod ollama;
mod log;
mod rag;
mod ui;
mod vector_store;
mod langchain_integration;
mod kb_builder;
mod config;

use anyhow::Result;
use app::{App, AppState};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ollama::OllamaClient;
use rag::RagPipeline;
use kb_builder::build_kb;
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use tokio::sync::mpsc;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use vector_store::VectorStore;
use std::env;

#[derive(Debug, Clone, Copy)]
enum AppCommand {
    Quit,
    Rebuild,
    Reload,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let cfg = config::load_config()?;
        let mut ollama = OllamaClient::new(cfg.ollama_model.clone());
        ollama.set_embed_model(cfg.embedding_model.clone());
        let packages = brew::get_installed_packages()?;
        let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();
        let query = args[1..].join(" ");
        match ollama.query(&query, &package_names, None).await {
            Ok(res) => {
                println!("{}", res);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load Homebrew packages
    let packages = brew::get_installed_packages()?;

    // Load config and initialize Ollama client
    let cfg = config::load_config()?;
    let mut ollama = OllamaClient::new(cfg.ollama_model.clone());
    ollama.set_embed_model(cfg.embedding_model.clone());

    // Initialize vector store (open DB now)
    let db_path = get_db_path()?;
    let vector_store = VectorStore::new(db_path.clone())?;

    // KB readiness flag and status channel
    let kb_ready = Arc::new(AtomicBool::new(!vector_store.is_empty()?));
    let rebuilding = Arc::new(AtomicBool::new(false));
    let reloading = Arc::new(AtomicBool::new(false));
    let (status_tx, mut status_rx) = mpsc::unbounded_channel::<String>();

    // Spawn background KB builder if not ready
    if !kb_ready.load(Ordering::SeqCst) {
        let db_path_clone = db_path.clone();
        let pkgs = packages.clone();
        let tx = status_tx.clone();
        let kb_flag = kb_ready.clone();
        let cfg_for_build = cfg.clone();

        // Use spawn_blocking + a current-thread runtime because build_kb uses non-Send types (rusqlite::Connection)
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build current-thread runtime for KB builder");

            rt.block_on(async move {
                if let Err(e) = build_kb(db_path_clone, pkgs, tx, kb_flag.clone(), cfg_for_build).await {
                    crate::log::log_error(&format!("Background KB build failed: {}", e));
                }
            });
        });
    }

    // Create app
    let mut app = App::new();

    // Remove synchronous indexing; background builder handles it

    app.set_status(None);
    app.clear_input();

    // Run the app loop
    loop {
        // Drain status messages from builder (non-blocking) and show in UI
        while let Ok(msg) = status_rx.try_recv() {
            app.set_status(Some(msg));
            terminal.draw(|f| ui::render(f, &app))?;
        }

        let cmd = run_app(&mut terminal, &mut app, &ollama, &db_path, kb_ready.clone(), rebuilding.clone(), reloading.clone(), &packages).await?;

        match cmd {
            AppCommand::Quit => break,
            AppCommand::Rebuild => {
                app.set_status(Some("Rebuild started in background".to_string()));
                kb_ready.store(false, Ordering::SeqCst);
                rebuilding.store(true, Ordering::SeqCst);
                let db_path_clone = db_path.clone();
                let pkgs = packages.clone();
                let tx = status_tx.clone();
                let kb_flag = kb_ready.clone();
                let rebuilding_flag = rebuilding.clone();
                let cfg_clone = cfg.clone();
                tokio::task::spawn_blocking(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to build current-thread runtime for KB rebuild");
                    rt.block_on(async move {
                        let _ = tx.send("Rebuilding knowledge base...".to_string());
                        if let Err(e) = build_kb(db_path_clone, pkgs, tx.clone(), kb_flag.clone(), cfg_clone).await {
                            crate::log::log_error(&format!("Background rebuild failed: {}", e));
                            let _ = tx.send(format!("Error rebuilding: {}", e));
                        }
                        rebuilding_flag.store(false, Ordering::SeqCst);
                    });
                });
                app.clear_input();
            }
            AppCommand::Reload => {
                app.set_status(Some("Reloading index data in background...".to_string()));
                reloading.store(true, Ordering::SeqCst);
                let tx = status_tx.clone();
                let reloading_flag = reloading.clone();
                tokio::task::spawn(async move {
                    let _ = tx.send("Reloading index data...".to_string());
                    // Simple simulate reopen; queries open fresh connections anyway
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    let _ = tx.send("Index reloaded.".to_string());
                    reloading_flag.store(false, Ordering::SeqCst);
                });
            }
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
    ollama: &OllamaClient,
    db_path: &PathBuf,
    kb_ready: Arc<AtomicBool>,
    rebuilding: Arc<AtomicBool>,
    reloading: Arc<AtomicBool>,
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
                KeyCode::Char('r') | KeyCode::Char('R') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Ctrl+Shift+R => reload index data
                    return Ok(AppCommand::Reload);
                }
                KeyCode::Char('r') | KeyCode::Char('R') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // plain 'r' or 'R' triggers rebuild
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

                        let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();

                        if kb_ready.load(Ordering::SeqCst) && !rebuilding.load(Ordering::SeqCst) && !reloading.load(Ordering::SeqCst) {
                            // KB ready: open vector store once and reuse for this query
                            match VectorStore::new(db_path.clone()) {
                                Ok(vs) => {
                                    let rag = RagPipeline::new(&vs, ollama);
                                    match rag.query_with_rag(&query, &package_names, 2).await {
                                        Ok(response) => {
                                            app.set_response(response);
                                            app.clear_input();
                                        }
                                        Err(e) => {
                                            app.set_response(format!("Error: {}", e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    crate::log::log_error(&format!("Failed to open vector store for query: {}", e));
                                    match ollama.query(&query, &package_names, None).await {
                                        Ok(response) => {
                                            app.set_response(response);
                                            app.clear_input();
                                        }
                                        Err(e) => {
                                            app.set_response(format!("Error: {}", e));
                                        }
                                    }
                                }
                            }
                        } else {
                            // KB not ready or busy: directly query local Ollama without RAG
                            match ollama.query(&query, &package_names, None).await {
                                Ok(response) => {
                                    app.set_response(response);
                                    app.clear_input();
                                }
                                Err(e) => {
                                    app.set_response(format!("Error: {}", e));
                                }
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

#[allow(dead_code)]
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

    // Process docs in batches of 10 for better efficiency
    let batch_size = 5;
    for batch_start in (0..total).step_by(batch_size) {
        let batch_end = std::cmp::min(batch_start + batch_size, total);
        let batch = &docs[batch_start..batch_end];
        
        for doc in batch {
            match ollama.generate_embedding(&doc.man_content).await {
                Ok(embedding) => {
                    if let Err(e) = vector_store.store_command(
                        &doc.package_name,
                        &doc.command_name,
                        &doc.man_content,
                        &embedding,
                    ) {
                        crate::log::log_error(&format!("Failed to store: {}: {}", doc.command_name, e));
                    }
                }
                Err(e) => {
                    crate::log::log_error(&format!("Failed to embed: {}: {}", doc.command_name, e));
                }
            }
        }

        // Update status
        app.set_status(Some(format!("Rebuilding: {}/{} commands", batch_end, total)));
        terminal.draw(|f| ui::render(f, app))?;
        
        // Yield CPU to prevent UI blocking during KB rebuild
        // 50ms is optimal: long enough to batch process, short enough for responsive UI
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    let count = vector_store.count()?;
    app.set_status(Some(format!("Knowledge base rebuilt! {} commands indexed.", count)));
    terminal.draw(|f| ui::render(f, app))?;

    // Small delay so user sees completion message
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    app.set_status(None);

    Ok(())
}
