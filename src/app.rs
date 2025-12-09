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
