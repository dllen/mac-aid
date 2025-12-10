pub enum AppState {
    Input,
    Loading,
}

pub struct App {
    pub state: AppState,
    pub input: String,
    pub response: String,
    pub should_quit: bool,
    // Status message for indexing or other operations
    pub status: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Input,
            input: String::new(),
            response: String::new(),
            should_quit: false,
            status: None,
        }
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
    }

    pub fn set_loading(&mut self) {
        self.state = AppState::Loading;
    }

    pub fn set_response(&mut self, response: String) {
        self.response = response;
    }

    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
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
