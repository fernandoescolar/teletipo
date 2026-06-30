/// Backend-independent rendering components.
/// Each component emits Scene commands without calling OpenGL or knowing about the renderer.

pub mod background;
pub mod command_palette;
pub mod context_menu;
pub mod cursor;
pub mod dropdown;
pub mod editor;
pub mod highlights;
pub mod overlay;
pub mod panel;
pub mod scrollbar;
pub mod search_panel;
pub mod selection;
pub mod suggestion;
pub mod tab_bar;
pub mod terminal;
pub mod toast;

pub use background::Background;
pub use editor::Editor;
pub use terminal::Terminal;
pub use toast::render as render_toasts;
pub use highlights::render as render_highlights;
pub use selection::render as render_selection;
pub use cursor::render as render_cursor;
pub use scrollbar::render as render_scrollbar;
pub use suggestion::render as render_suggestion;
pub use tab_bar::render as render_tab_bar;
pub use search_panel::render as render_search_panel;
pub use command_palette::render as render_command_palette;
pub use context_menu::render as render_context_menu;
pub use dropdown::render as render_dropdown;
