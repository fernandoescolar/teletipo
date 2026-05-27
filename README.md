# Teletipo

TeleTipo is a modern terminal emulator written in Rust, inspired by classic teletypes and retro computing aesthetics.

It combines GPU-accelerated rendering, multi-tab workflows, rich color support, and a code-editor-like command input to create a terminal experience that feels fast, expressive, and playful.

![Teletipo screenshot](docs/teletipo128x128.png)

## Current Stable Baseline (v0.1)

This first stable baseline focuses on a reliable terminal core and shell runtime:

- Real PTY integration via portable-pty
- Stateful ANSI/VT parser with key CSI/DEC sequences
- Primary and alternate screen buffers
- Scrollback support
- Cell styles with basic SGR handling
- Screen snapshots and damage tracking
- Command execution mode and shell mode from CLI
- GPU window runtime as the default execution path
- Cross-crate tests passing in the workspace

## Workspace Layout

- `crates/terminal-ansi`: ANSI/VT parser and action model
- `crates/terminal-screen`: Screen/grid model, styles, scrollback, damage tracking
- `crates/terminal-core`: Applies parser actions over screen state
- `crates/terminal-pty`: PTY abstractions and portable-pty session
- `crates/editor-core`: Basic editor buffer and undo/redo
- `crates/app-orchestrator`: Wires terminal/editor and PTY pump loop
- `crates/app-cli`: Application runtime library and CLI surface
- `src/main.rs`: Workspace binary entrypoint (`cargo run`)

## Build and Test

```bash
cargo test --workspace
```

Benchmark suites:

```bash
cargo bench -p terminal-ansi
cargo bench -p terminal-screen
```

## Run

Run the app (root entrypoint):

```bash
cargo run
```

Run with a startup command:

```bash
cargo run -- --exec "printf 'hello from teletipo\\n'"
```

Run the app crate directly (equivalent to root run):

```bash
cargo run -p app-cli
```

Note: GPU mode is always enabled now. The `--gpu` flag was removed.

## Release Builds

A top-level [Makefile](Makefile) is available to build release artifacts for macOS, Linux, and Windows.

Prerequisites:

- Rust targets for macOS are installed automatically by the Makefile.
- `cross` is required for Linux/Windows cross-compilation (`cargo install cross`).
- Docker is required by `cross`.

Commands:

```bash
# Build all release artifacts
make release

# Platform-specific builds
make release-macos
make release-linux
make release-windows

# Remove release artifacts
make clean
```

## Notes

This is a stable core milestone, not the final product. Next major milestones are GPU rendering (winit + wgpu), richer TUI compatibility, and integrated editor UX parity.

Detailed stable architecture is documented in `docs/STABLE_V1_ARCHITECTURE.md`.

Execution backlog for the next milestones is documented in `docs/ROADMAP_V1_1_TO_V1_5.md`.
