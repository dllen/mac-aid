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

#[derive(Debug, Clone, Copy)]
enum AppCommand {
    Quit,
    Rebuild,
    Reload,
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

    // Initialize vector store (open DB now)
    let db_path = get_db_path()?;
    let mut vector_store = VectorStore::new(db_path.clone())?;

    // KB readiness flag and status channel
    let kb_ready = Arc::new(AtomicBool::new(!vector_store.is_empty()?));
    let (status_tx, mut status_rx) = mpsc::unbounded_channel::<String>();

    // Spawn background KB builder if not ready
    if !kb_ready.load(Ordering::SeqCst) {
        let db_path_clone = db_path.clone();
        let pkgs = packages.clone();
        let tx = status_tx.clone();
        let kb_flag = kb_ready.clone();

        // Use spawn_blocking + a current-thread runtime because build_kb uses non-Send types (rusqlite::Connection)
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build current-thread runtime for KB builder");

            rt.block_on(async move {
                if let Err(e) = build_kb(db_path_clone, pkgs, tx, kb_flag.clone()).await {
                    crate::log::log_error(&format!("Background KB build failed: {}", e));
                }
            });
        });
    }

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

        // Process docs in batches of 10 for better efficiency
        let batch_size = 10;
        for batch_start in (0..total).step_by(batch_size) {
            let batch_end = std::cmp::min(batch_start + batch_size, total);
            let batch = &docs[batch_start..batch_end];
            
            // Prepare texts for batch embedding
            let texts: Vec<&str> = batch.iter().map(|d| d.man_content.as_str()).collect();
            
            match ollama.generate_embeddings_batch(&texts).await {
                Ok(embeddings) => {
                    for (doc, embedding) in batch.iter().zip(embeddings.iter()) {
                        if let Err(e) = vector_store.store_command(
                            &doc.package_name,
                            &doc.command_name,
                            &doc.man_content,
                            embedding,
                        ) {
                            crate::log::log_error(&format!("Failed to store: {}: {}", doc.command_name, e));
                        }
                    }
                }
                Err(e) => {
                    crate::log::log_error(&format!("Batch embedding failed (docs {}-{}): {}", batch_start, batch_end, e));
                    // Fall back to individual embedding for this batch
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
                }
            }

            // Update status
            app.set_status(Some(format!("Indexed {}/{} commands", batch_end, total)));
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

    // Run the app loop
    loop {
        // Drain status messages from builder (non-blocking) and show in UI
        while let Ok(msg) = status_rx.try_recv() {
            app.set_status(Some(msg));
            terminal.draw(|f| ui::render(f, &app))?;
        }

        let cmd = run_app(&mut terminal, &mut app, &ollama, &db_path, kb_ready.clone(), &packages).await?;

        match cmd {
            AppCommand::Quit => break,
            AppCommand::Rebuild => {
                // Rebuild knowledge base
                if let Err(e) = rebuild_knowledge_base(&mut vector_store, &ollama, &packages, &mut terminal, &mut app).await {
                    app.set_response(format!("Error rebuilding: {}", e));
                }
                app.clear_input();
            }
            AppCommand::Reload => {
                // Reload index data by re-opening the DB (recreate VectorStore)
                app.set_status(Some("Reloading index data...".to_string()));
                terminal.draw(|f| ui::render(f, &app))?;
                match VectorStore::new(db_path.clone()) {
                    Ok(new_vs) => {
                        vector_store = new_vs;
                        app.set_status(Some("Index reloaded.".to_string()));
                    }
                    Err(e) => {
                        app.set_status(Some(format!("Failed to reload index: {}", e)));
                        crate::log::log_error(&format!("Failed to reload index: {}", e));
                    }
                }
                terminal.draw(|f| ui::render(f, &app))?;
                // small pause so user sees status
                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                app.set_status(None);
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

                        if kb_ready.load(Ordering::SeqCst) {
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
                            // KB not ready: directly query local Ollama without RAG
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
    let batch_size = 10;
    for batch_start in (0..total).step_by(batch_size) {
        let batch_end = std::cmp::min(batch_start + batch_size, total);
        let batch = &docs[batch_start..batch_end];
        
        let texts: Vec<&str> = batch.iter().map(|d| d.man_content.as_str()).collect();
        
        match ollama.generate_embeddings_batch(&texts).await {
            Ok(embeddings) => {
                for (doc, embedding) in batch.iter().zip(embeddings.iter()) {
                    if let Err(e) = vector_store.store_command(
                        &doc.package_name,
                        &doc.command_name,
                        &doc.man_content,
                        embedding,
                    ) {
                        crate::log::log_error(&format!("Failed to store: {}: {}", doc.command_name, e));
                    }
                }
            }
            Err(e) => {
                crate::log::log_error(&format!("Batch embedding failed (docs {}-{}): {}", batch_start, batch_end, e));
                // Fall back to individual embedding for this batch
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
