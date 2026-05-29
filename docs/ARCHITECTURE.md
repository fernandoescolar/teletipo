# Teletipo Architecture (As Built)

This document describes the current architecture implemented in the workspace.
It is intentionally descriptive (current state), not aspirational.

## Workspace Layout

The repository is a Rust workspace with a thin root binary and focused crates:

- `src/main.rs`: process entrypoint and app startup wiring.
- `crates/app-cli`: main runtime loop, event handling, snapshot construction, config/theme/session load/save, updater integration.
- `crates/app-orchestrator`: app orchestration model and runtime event contracts.
- `crates/editor-core`: editor buffer engine, cursor/selection operations, history, semantic command extraction.
- `crates/editor-lang`: token-oriented highlighter traits and shell-like highlighter implementation.
- `crates/terminal-ansi`: ANSI action model and parser.
- `crates/terminal-screen`: terminal grid, damage tracking, and snapshot generation.
- `crates/terminal-core`: parser/screen session boundary (`TerminalParser` and `TerminalDisplay`) and default session type.
- `crates/terminal-pty`: PTY backend abstraction and process session glue.
- `crates/render-wgpu`: WGPU rendering pipeline, glyph atlas handling, geometry conversion, and GPU window execution.
- `crates/platform-abstraction`: platform traits and implementations for clipboard, window control, process metadata, DPI, IME, and accessibility.
- `crates/ui`: UI state/actions/input abstractions shared by app and render layers.

## Runtime Data Flow

1. `main` starts `app-cli`.
2. `app-cli` initializes configuration, theme, shell/session context, and runtime services.
3. PTY output is read through `terminal-pty` and forwarded to `terminal-core`.
4. `terminal-core` advances parser/screen state (`terminal-ansi` + `terminal-screen`).
5. `app-cli` builds a `RenderSnapshot` from terminal/editor/UI state.
6. `render-wgpu` converts snapshot data into GPU vertex streams and presents through WGPU.
7. User input events are routed back through app handlers and into editor/terminal actions.

## Threading Model

Current implementation is primarily synchronous and event-loop driven:

- UI/render loop runs on the main thread.
- PTY I/O is handled by a dedicated worker thread/session backend.
- Shared state is exchanged via channels and snapshot copies optimized for render reads.

No Tokio runtime is required for the core path today.

## Rendering Notes

`render-wgpu` currently uses:

- Snapshot-driven CPU-side vertex generation.
- A glyph atlas texture for text raster cache.
- Damage-aware rendering interfaces (row-granular today).
- Bounded atlas cache behavior with LRU-style eviction and miss-rate driven repack heuristics.

## Platform Layer

Platform-specific behavior is centralized in `platform-abstraction`:

- macOS window icon/titlebar color operations.
- process metadata (`ProcessInfo`) such as cwd lookup by pid.
- clipboard, IME, accessibility, DPI, and font fallback traits.

This keeps `app-cli` and `render-wgpu` free from direct platform FFI details.

## Architecture Reality vs. Earlier Blueprint

Key differences from earlier planning notes:

- Editor buffer engine is gap-buffer based today (not rope-first).
- Highlighting is shell-token style and trait-based (not tree-sitter-first in the active path).
- Runtime concurrency is std-thread based around the PTY path (not Tokio-centric).
- Platform code is organized through shared trait and adapter modules rather than a deep per-OS folder tree.

## Observability and Quality Gates

The workspace currently uses:

- `tracing` for structured runtime diagnostics.
- `metrics` counters/histograms around key runtime subsystems.
- CI and local gates for format, lint, docs, and tests.

## Current Known Design Debt

Active backlog items still track larger architectural shifts:

- `T16`: redesign `RenderSnapshot` and damage model for stronger perf and API coherence.
- Remaining documentation debt behind `missing_docs` lint rollout.

When these tasks land, this document should be updated in the same PR/commit as the architectural change.
