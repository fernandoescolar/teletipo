/// Backend-independent rendering components.
/// Each component emits Scene commands without calling OpenGL or knowing about the renderer.

pub mod background;
pub mod cursor;
pub mod editor;
pub mod highlights;
pub mod overlay;
pub mod panel;
pub mod scrollbar;
pub mod selection;
pub mod suggestion;
pub mod tab_bar;
pub mod terminal;
pub mod toast;

pub use background::Background;
pub use editor::Editor;
pub use tab_bar::TabBar;
pub use terminal::Terminal;
pub use toast::render as render_toasts;
pub use highlights::render as render_highlights;
pub use selection::render as render_selection;
pub use cursor::render as render_cursor;
pub use scrollbar::render as render_scrollbar;
pub use suggestion::render as render_suggestion;
