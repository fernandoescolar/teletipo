![Teletipo logo](docs/teletipo128x128.png)

# Teletipo

A modern terminal emulator written in Rust — GPU-accelerated, multi-tab, with rich color support and a code-editor-like command input.

![Teletipo screenshot](docs/teletipo-screenshot.gif)


## Features

- GPU-accelerated rendering via `wgpu`
- Multi-tab workflow with tab reordering, middle-click close, and per-tab unread/bell badges
- Stateful ANSI/VT100 parser (CSI, DEC sequences, SGR, alternate screen, scrollback)
- Primary and alternate screen buffers
- Inline terminal search panel (`Cmd+F` / `Ctrl+F`) with match highlights and next/previous navigation
- Search scope includes both visible terminal rows and scrollback history
- Code-editor-style command input
- Command palette (`Cmd+Shift+P`) for common actions such as tab management, settings, config actions, and prompt navigation
- Prompt-aware navigation using OSC 133 shell markers (`Jump to Previous Prompt` / `Jump to Next Prompt`)
- Right-click context menus for tabs and the terminal pane
- Drag-and-drop support for applying YAML themes or pasting file paths into the command editor
- Scrollback activity indicator with quick jump back to bottom
- Themeable (YAML theme files, ships with Catppuccin Mocha, Dracula, Gruvbox, Nord, One Dark, Rosé Pine, Solarized Dark, Tokyo Night)
- Automatic silent self-update from GitHub Releases

## Before install

For the best experience with Teletipo, we recommend installing the following first:

- At least one Nerd Font: https://www.nerdfonts.com/font-downloads (I use Hack Nerd Font)
- Starship prompt: https://github.com/starship/starship

To use my Starship preset:

	starship preset gruvbox-rainbow -o ~/.config/starship.toml

## Install

### Quick install (recommended)

#### macOS + Linux

```bash
curl -fsSL https://github.com/fernandoescolar/teletipo/releases/latest/download/install.sh | sh
```

On macOS, the installer now defaults to desktop mode. Use `--no-desktop` if you want the CLI-only install.

Install as desktop app too:

```bash
curl -fsSL https://github.com/fernandoescolar/teletipo/releases/latest/download/install.sh | sh -s -- --desktop
```

#### Windows (PowerShell)

```powershell
irm https://github.com/fernandoescolar/teletipo/releases/latest/download/install.ps1 | iex
```

### Verified/manual install

```bash
curl -fsSLO https://github.com/fernandoescolar/teletipo/releases/latest/download/teletipo-linux-x86_64.tar.gz
curl -fsSLO https://github.com/fernandoescolar/teletipo/releases/latest/download/SHA256SUMS
grep ' teletipo-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum -c -
tar -xzf teletipo-linux-x86_64.tar.gz
./teletipo-linux-x86_64/install.sh --desktop
```

Each release archive includes platform-native installer scripts:

- Linux/macOS: `install.sh` and `uninstall.sh`
- Windows: `install.ps1` and `uninstall.ps1`

### Platform archives

- macOS CLI (universal): `teletipo-macos-universal.tar.gz`
- macOS app bundle: `teletipo-macos-app.tar.gz`
- Linux x86-64: `teletipo-linux-x86_64.tar.gz`
- Windows x86-64: `teletipo-windows-x86_64.zip`

### Build from source

```bash
git clone https://github.com/fernandoescolar/teletipo.git
cd teletipo
cargo build --release
# binary at target/release/teletipo
```

## Auto-update

Teletipo updates itself automatically. Every time it launches, a background thread checks GitHub Releases for a newer version. Release archives are signed and verified before replacement. If one is found the new binary is downloaded and replaces the current one silently — no prompts, no flags. When the update is complete a brief overlay appears in the terminal window:

```
Updated to vX.Y.Z — restart to apply
```

Simply close and reopen Teletipo to start using the new version.

If you ever need to roll back to the previous executable after a bad update, run:

```bash
teletipo update rollback
```

> The update check is non-blocking: the terminal is fully usable while the download happens in the background. If the check fails (no network, GitHub unreachable, etc.) the app starts normally with no error.

## Usage

```bash
# Open a shell in the current directory
teletipo

# Run a specific command on startup
teletipo --exec "htop"

# Expose Prometheus metrics on 127.0.0.1:9898
teletipo --metrics

# Increase runtime logging verbosity
RUST_LOG=app_cli=debug,render_wgpu=info teletipo
```

Runtime logs are written automatically to daily-rotated files under the platform data directory (see [File Locations](#file-locations)).

## Keyboard Shortcuts

> `Cmd` on macOS · `Super`/`Win` on Linux/Windows

### Tabs

| Shortcut | Action |
|---|---|
| `Cmd + T` | New tab |
| `Cmd + W` | Close current tab |
| `Cmd + [` | Previous tab |
| `Cmd + ]` | Next tab |
| `Cmd + 1 – 9` | Jump to tab N |

### Command palette

| Shortcut | Action |
|---|---|
| `Cmd + Shift + P` | Open command palette |
| `↑` / `↓` | Move through commands |
| `Enter` | Execute highlighted command |
| `Escape` | Close the palette |
| typing / `Backspace` | Filter commands live |

### Command input (editor)

| Shortcut | Action |
|---|---|
| `Enter` | Submit command |
| `Shift + Enter` | Insert newline (multi-line input) |
| `↑` / `↓` | Navigate command history (when cursor is on first/last line) |
| `↑` / `↓` | Move cursor up/down in multi-line input |
| `←` / `→` | Move cursor left/right |
| `Shift + ← / →` | Extend selection |
| `Home` | Jump to start of input |
| `End` | Jump to end of input |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character after cursor |

### Suggestions (autocomplete dropdown)

Press `Tab` at the end of any line to open the suggestion popup, which shows history entries ranked by recency and frequency that match what you have typed so far. The selected entry is previewed in gray ghost text — nothing is written to the editor until you confirm.

| Shortcut | Action |
|---|---|
| `Tab` | Open popup (first suggestion) · confirm highlighted entry |
| `Shift + Tab` | Open popup (last suggestion) · cycle backward |
| `↑` / `↓` | Navigate up / down through the list |
| `Enter` | Confirm highlighted entry and submit the command |
| `Escape` | Dismiss popup (editor text unchanged) |
| typing / `Backspace` | Refilter the list live as you edit the prefix |

### Copy & paste

| Shortcut | Action |
|---|---|
| `Cmd + C` / `Ctrl + Shift + C` | Copy terminal selection or editor selection (`Cmd` on macOS, `Ctrl + Shift` on Linux/Windows) |
| `Cmd + V` / `Ctrl + Shift + V` | Paste from clipboard (`Cmd` on macOS, `Ctrl + Shift` on Linux/Windows) |

### Terminal scrollback

| Shortcut | Action |
|---|---|
| `Page Up` | Scroll up 5 lines |
| `Page Down` | Scroll down 5 lines |

### Terminal search

| Shortcut | Action |
|---|---|
| `Cmd + F` / `Ctrl + F` | Open search panel for the active tab |
| typing | Update search query |
| `Backspace` | Delete previous character in query |
| `Enter` / `↓` | Jump to next match |
| `↑` | Jump to previous match |
| `Escape` | Close search panel |

### Settings

| Shortcut | Action |
|---|---|
| `Cmd + ,` | Open settings panel |
| `Ctrl + ,` | Open settings panel |

### Terminal control sequences

| Shortcut | Sends |
|---|---|
| `Ctrl + A – Z` | `^A` – `^Z` (control characters) |
| `Ctrl + [` | `ESC` |
| `Escape` | `ESC` (or dismiss suggestion popup — see above) |
| `Tab` | Open/confirm suggestion popup (see above) |

### Mouse

| Action | Effect |
|---|---|
| Click + drag in terminal | Select text |
| Copy shortcut after selection | Copy selection (`Cmd + C` on macOS, `Ctrl + Shift + C` on Linux/Windows) |
| Click search panel buttons | Previous match, next match, or close search |
| Drag the split divider | Resize terminal / editor pane |
| Drag the scrollbar | Scroll terminal or editor |
| Middle-click a tab | Close that tab |
| Right-click a tab | Tab context menu |
| Right-click in terminal | Terminal context menu (`Copy`, `Paste`, `Scroll to Bottom`) |
| Click the scroll activity badge | Jump back to the bottom of scrollback |
| Drop a `*.yaml` file on the window | Apply that theme immediately |
| Drop any other file on the window | Paste its path into the command editor |

## Settings

Open the settings panel with `Cmd+,` (or `Ctrl+,`). All changes are saved automatically when you close the panel with `Escape`.

The settings panel also includes quick actions to open the active `config.toml` in your editor or reveal it in Finder / your platform file manager.

### Navigation

| Key | Action |
|---|---|
| `↑` / `↓` | Move focus between settings fields |
| `←` / `→` | Cycle through selectable values (theme, font family) or increment/decrement numeric fields |
| `Enter` | Open search mode for searchable fields (theme, font family); open direct-edit mode for all other fields |
| `Escape` | Close panel and save; or cancel the current edit / search without saving |
| `Cmd + S` | Save the current field edit immediately |

### Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `theme` | Selector | `tokyo-night` | Active color theme. Use `←`/`→` to cycle through built-in and custom themes, or press `Enter` to open a live search by typing part of the name. |
| `font › size` | Numeric (±0.5 pt) | `16` | Font point size. Use `←`/`→` to increment by 0.5, or press `Enter` to type a value directly. |
| `font › family` | Selector + Search | *(system default)* | Font family name, resolved via the system font database. Use `←`/`→` to cycle alphabetically, or press `Enter` then type to filter. Select a result with `↑`/`↓` and confirm with `Enter`. |
| `padding › horizontal` | Numeric (±1 px) | `8` | Horizontal padding in pixels between the window edge and the terminal/editor content. Use `←`/`→` or press `Enter` to type directly. |
| `padding › vertical` | Numeric (±1 px) | `8` | Vertical padding in pixels. Use `←`/`→` or press `Enter` to type directly. |
| `terminal › shell` | Text | *(auto)* | Path to the shell executable (e.g. `/bin/zsh`). Leave empty to use the system default (`$SHELL`). Press `Enter` to edit, `Enter` again to confirm, `Escape` to cancel. |
| `terminal › scrollback_lines` | Numeric (±500 lines) | `10000` | Number of lines to keep in the scrollback buffer. Use `←`/`→` to adjust in steps of 500, or press `Enter` to type a value directly. Set to `0` for the compiled-in default. |

### Config file

All settings are persisted in `config.toml` in your platform config directory (see [File Locations](#file-locations)). You can edit this file directly with any text editor — changes are picked up on next launch.

```toml
[font]
size   = 16.0
family = "JetBrains Mono"   # omit to use the system default

[padding]
horizontal = 8
vertical   = 8

[terminal]
shell           = ""          # empty = $SHELL default
scrollback_lines = 10000

active_theme = "tokyo-night"
```

### Custom themes

Drop any `*.yaml` file into the `themes/` config directory and it will appear immediately in the theme selector. You can also drag a theme file directly onto the Teletipo window to apply it immediately. See the bundled theme files in the `themes/` folder for the format reference.

## File Locations

Teletipo stores its files in the standard OS directories via the [`dirs`](https://crates.io/crates/dirs) crate.

| What | File | macOS | Linux | Windows |
|---|---|---|---|---|
| Configuration | `config.toml` | `~/Library/Application Support/teletipo/` | `~/.config/teletipo/` | `%APPDATA%\teletipo\` |
| Custom themes | `themes/*.yaml` | `~/Library/Application Support/teletipo/themes/` | `~/.config/teletipo/themes/` | `%APPDATA%\teletipo\themes\` |
| Session & history | `session.json` | `~/Library/Application Support/teletipo/` | `~/.local/share/teletipo/` | `%LOCALAPPDATA%\teletipo\` |
| Logs | `logs/teletipo.log.YYYY-MM-DD` | `~/Library/Application Support/teletipo/logs/` | `~/.local/share/teletipo/logs/` | `%LOCALAPPDATA%\teletipo\logs\` |

**Configuration** (`config.toml`) — font, font size, theme, padding, shell, and other settings. Created with defaults on first launch.

**Custom themes** — drop any `*.yaml` theme file into the `themes/` directory and it will appear in the settings panel alongside the built-in themes.

**Session & history** (`session.json`) — command history with frecency data, and terminal output snapshots. Written on exit, restored on next launch.

**Logs** (`logs/teletipo.log.YYYY-MM-DD`) — structured runtime logs from all crates. A new file is created each day automatically.

## Workspace Layout

| Crate | Purpose |
|---|---|
| `src/main.rs` | Binary entry point |
| `crates/app-cli` | Application runtime library (GPU window, tabs, themes, updater, search overlay state/input routing) |
| `crates/app-orchestrator` | Wires terminal, editor, and PTY pump loop |
| `crates/editor-core` | Editor buffer and undo/redo |
| `crates/editor-lang` | Token-based syntax highlighting helpers |
| `crates/platform-abstraction` | Cross-platform adapters (clipboard, window control, process metadata, IME/accessibility) |
| `crates/render-wgpu` | WGPU renderer, snapshot-to-geometry conversion, text/background overlay rendering |
| `crates/terminal-ansi` | ANSI/VT parser and action model |
| `crates/terminal-screen` | Screen/grid model, styles, scrollback, damage tracking |
| `crates/terminal-core` | Applies parser actions over screen state |
| `crates/terminal-pty` | PTY abstractions and session management |
| `crates/ui` | Shared UI input/state primitives used across app and renderer |

## Development

```bash
# Run tests
cargo test --workspace

# Run benchmarks
cargo bench -p terminal-ansi
cargo bench -p terminal-screen

# Run from source
cargo run
cargo run -- --exec "printf 'hello\\n'"
```

## Release Builds

A [Makefile](Makefile) builds release artifacts for all platforms. Linux and Windows require [`cross`](https://github.com/cross-rs/cross) and Docker.

```bash
cargo install cross          # one-time setup

make release                 # all platforms
make release-macos           # macOS universal binary
make release-linux           # Linux x86-64
make release-windows         # Windows x86-64
make clean
```

Artifacts land in `dist/`:

| File | Platform |
|---|---|
| `teletipo-macos-universal.tar.gz` | macOS (arm64 + x86-64 lipo'd) |
| `teletipo-linux-x86_64.tar.gz` | Linux x86-64 |
| `teletipo-windows-x86_64.zip` | Windows x86-64 |
