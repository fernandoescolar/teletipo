//! Text selection and shell suggestion state shared by terminal/editor panes.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScrollState {
    pub terminal_offset: usize,
    pub editor_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub row: usize,
    pub col: usize,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub anchor: Option<SelectionPoint>,
    pub end: Option<SelectionPoint>,
    pub in_progress: bool,
}

impl SelectionState {
    pub fn begin(&mut self, point: SelectionPoint) {
        self.anchor = Some(point);
        self.end = Some(point);
        self.in_progress = true;
    }

    pub fn update(&mut self, point: SelectionPoint) {
        if self.in_progress {
            self.end = Some(point);
        }
    }

    pub fn finalize(&mut self) {
        self.in_progress = false;
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.end = None;
        self.in_progress = false;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SuggestionState {
    pub prefix: Option<String>,
    pub index: Option<usize>,
}

impl SuggestionState {
    pub fn clear(&mut self) {
        self.prefix = None;
        self.index = None;
    }
}
