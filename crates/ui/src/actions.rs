use crate::components::ModifierState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorCmd {
    Backspace,
    DeleteForward,
    MoveLeft {
        extend_selection: bool,
    },
    MoveRight {
        extend_selection: bool,
    },
    SetCursor {
        offset: usize,
        extend_selection: bool,
    },
    Undo,
    Redo,
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsCmd {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,
    Home,
    End,
    BeginEdit,
    InsertChar(char),
    Backspace,
    Delete,
    CommitEdit,
    CancelEdit,
    Save,
    OpenSearch,
    CloseSearch,
    SearchInsertChar(char),
    SearchBackspace,
    SearchNext,
    SearchPrev,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    NewTab,
    CloseTab(usize),
    SwitchTab(usize),
    MoveTab {
        from: usize,
        to: usize,
    },
    DragTabStart(usize),
    DragTabUpdate {
        cursor_x: f64,
    },
    DragTabEnd,
    OpenTabContextMenu {
        tab: usize,
        x: f64,
        y: f64,
    },
    CloseContextMenu,
    ContextMenuHover(Option<usize>),
    RenameTab(usize, String),

    ToggleFocus,
    SetSplitRatio(f32),
    ToggleFullscreen,

    SendToTerminal(Vec<u8>),
    EditorInsert(String),
    EditorAction(EditorCmd),
    SendReturn,

    ScrollBy(i32),
    ScrollTo(usize),
    EditorScrollBy(i32),

    SelectionBegin {
        row: usize,
        col: usize,
    },
    SelectionUpdate {
        row: usize,
        col: usize,
    },
    SelectionEnd,
    ClearSelection,
    CopySelection,

    OpenSettings,
    CloseSettings,
    SettingsAction(SettingsCmd),

    CycleSuggestion(i32),
    AcceptSuggestion,
    ClearSuggestion,

    Paste(String),
    RequestExit,
    ToggleCursorBlink,

    Resized {
        width: u32,
        height: u32,
        scale: f64,
        cell_w: f32,
        cell_h: f32,
    },
    CursorMoved {
        x: f64,
        y: f64,
    },
    MouseWheel(f32),
    ModifiersChanged(ModifierState),
}
