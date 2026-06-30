/// Backend-independent rendering components.
/// Each component emits Scene commands without calling OpenGL or knowing about the renderer.

pub mod background;
pub mod editor;
pub mod overlay;
pub mod panel;
pub mod tab_bar;
pub mod terminal;
pub mod toast;

pub use background::Background;
pub use editor::Editor;
pub use tab_bar::TabBar;
pub use terminal::Terminal;
