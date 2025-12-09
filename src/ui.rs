use crate::app::{App, AppState};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    widgets::Gauge,
    Frame,
};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(f.area());

    render_package_list(f, app, chunks[0]);
    render_right_panel(f, app, chunks[1]);
}

fn render_package_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .packages
        .iter()
        .enumerate()
        .map(|(i, pkg)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(pkg.name.clone()).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title("📦 Homebrew Packages")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(list, area);
}

fn render_right_panel(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_input(f, app, chunks[0]);
    render_progress(f, app, chunks[1]);
    render_response(f, app, chunks[2]);
}

fn render_progress(f: &mut Frame, app: &App, area: Rect) {
    if let (Some(current), Some(total)) = (app.progress_current, app.progress_total) {
        let ratio = if total == 0 {
            0.0
        } else {
            (current as f64) / (total as f64)
        };

        let msg = app
            .progress_message
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("");

        let gauge = Gauge::default()
            .block(
                Block::default()
                    .title("🔁 Indexing Progress")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .gauge_style(Style::default().fg(Color::Yellow))
            .ratio(ratio);

        f.render_widget(gauge, area);

        // overlay a small paragraph with text about progress
        let info = Paragraph::new(format!("{}/{} {}", current, total, msg))
            .style(Style::default().fg(Color::White));
        f.render_widget(info, area);
    } else {
        // render empty block to keep layout consistent
        let block = Block::default().borders(Borders::ALL);
        f.render_widget(block, area);
    }
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    let input_text = match app.state {
        AppState::Input => format!("{}_", app.input),
        AppState::Loading => "Loading...".to_string(),
        AppState::Normal => app.input.clone(),
    };

    let style = match app.state {
        AppState::Input => Style::default().fg(Color::Green),
        AppState::Loading => Style::default().fg(Color::Yellow),
        AppState::Normal => Style::default().fg(Color::White),
    };

    let input = Paragraph::new(input_text)
        .style(style)
        .block(
            Block::default()
                .title("🔍 Query (Press 'i' to input, Enter to submit, Esc to cancel)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(input, area);
}

fn render_response(f: &mut Frame, app: &App, area: Rect) {
    let text = if app.response.is_empty() {
        Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "💡 Press 'i' to enter a query",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Example: \"I need to process JSON files\"",
                Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Controls:",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  ↑/↓  - Navigate packages",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  i    - Enter input mode",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  Esc  - Exit input mode",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  q    - Quit application",
                Style::default().fg(Color::Gray),
            )),
        ])
    } else {
        Text::from(app.response.clone())
    };

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("🤖 AI Response")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}
