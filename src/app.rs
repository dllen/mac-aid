use crate::brew::BrewPackage;

pub enum AppState {
    Normal,
    Input,
    Loading,
}

pub struct App {
    pub packages: Vec<BrewPackage>,
    pub selected_index: usize,
    pub state: AppState,
    pub input: String,
    pub response: String,
    pub should_quit: bool,
    // Progress for indexing (current / total)
    pub progress_current: Option<usize>,
    pub progress_total: Option<usize>,
    pub progress_message: Option<String>,
}

impl App {
    pub fn new(packages: Vec<BrewPackage>) -> Self {
        Self {
            packages,
            selected_index: 0,
            state: AppState::Normal,
            input: String::new(),
            response: String::new(),
            should_quit: false,
            progress_current: None,
            progress_total: None,
            progress_message: None,
        }
    }

    pub fn next(&mut self) {
        if self.packages.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.packages.len();
    }

    pub fn previous(&mut self) {
        if self.packages.is_empty() {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = self.packages.len() - 1;
        }
    }

    pub fn enter_input_mode(&mut self) {
        self.state = AppState::Input;
        self.input.clear();
    }

    pub fn exit_input_mode(&mut self) {
        self.state = AppState::Normal;
    }

    pub fn set_loading(&mut self) {
        self.state = AppState::Loading;
    }

    pub fn set_response(&mut self, response: String) {
        self.response = response;
        self.state = AppState::Normal;
    }

    pub fn set_progress(&mut self, total: usize) {
        self.progress_total = Some(total);
        self.progress_current = Some(0);
        self.progress_message = None;
    }

    pub fn update_progress(&mut self, current: usize, message: Option<String>) {
        self.progress_current = Some(current);
        if let Some(m) = message {
            self.progress_message = Some(m);
        }
    }

    pub fn clear_progress(&mut self) {
        self.progress_current = None;
        self.progress_total = None;
        self.progress_message = None;
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
    }

    pub fn pop_char(&mut self) {
        self.input.pop();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}
