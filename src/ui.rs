use crate::app::{App, AppState};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // input box
            Constraint::Length(1),  // status line
            Constraint::Min(0),     // response area
        ])
        .split(f.area());

    render_input(f, app, chunks[0]);
    render_status(f, app, chunks[1]);
    render_response(f, app, chunks[2]);
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    let input_text = match app.state {
        AppState::Input => format!("{}_", app.input),
        AppState::Loading => "Loading...".to_string(),
    };

    let style = match app.state {
        AppState::Input => Style::default().fg(Color::Green),
        AppState::Loading => Style::default().fg(Color::Yellow),
    };

    let input = Paragraph::new(input_text)
        .style(style)
        .block(
            Block::default()
                .title("🔍 Enter your need")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    f.render_widget(input, area);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let status_text = if let Some(status) = &app.status {
        status.clone()
    } else {
        "Ready".to_string()
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(status, area);
}

fn render_response(f: &mut Frame, app: &App, area: Rect) {
    let text = if app.response.is_empty() {
        Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "💡 Type your need and press Enter",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Example: \"How to compress a file?\"",
                Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'q' to quit, 'r' to rebuild, ↑↓ to scroll",
                Style::default().fg(Color::Gray),
            )),
        ])
    } else {
        Text::from(app.response.clone())
    };

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("💡 Recommendation")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true })
        .scroll((app.scroll_offset, 0));

    f.render_widget(paragraph, area);
}
