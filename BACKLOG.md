### Findings

| # | Sev | Area | Location | Evidence | Why it matters | Fix | Fixed |
|---|-----|------|---------|----------|----------------|-----|-------|
| **SEC-1** | P0 | Security / supply chain | updater.rs, main.rs | `Update::configure()...update().ok()?` then `tx.send(try_update()).ok();`. `self_update` 0.44 with `ureq`+`rustls`, no minisign/sha256/pin, no rollback | A tampered GitHub release or rustls-trusted MITM silently replaces the running binary | Sign artifacts (minisign or sigstore), verify sig+sha256 pre-swap, keep `<exe>.bak`, surface failures in UI | Yes (T1) |
| **SEC-2** | P0 | Security / DoS | launch.rs | `App::new(rows, cols).expect("valid terminal size")` with rows/cols from user-writable session JSON | Crafted file → panic or multi-GB `Vec<Cell>` allocation | Clamp 1..=1024/1..=4096; warn + fall back to defaults | Yes (T2) |
| **SEC-3** | P0 | Security | Cargo.toml workspace lints `unsafe_code = "warn"`; cast at geometry.rs | `unsafe { slice::from_raw_parts(v.as_ptr() as *const u8, v.len()*4) }` — preventable | One avoidable unsafe in a 1.3k-line file; policy is too permissive | `unsafe_code = "forbid"`, allow per-file in `window.rs`/`coords.rs`, replace cast with `bytemuck::cast_slice` | Yes (T3; enforced as `deny` to allow scoped FFI exceptions) |
| **LIC-1** | P0 | Legal | repo root | No LICENSE; no per-crate `license` field | Code is "all rights reserved" by default; CI publishes binaries but grants no rights to users | Add MIT OR Apache-2.0 + per-crate metadata | Yes (T13) |
| **REL-1** | P1 | Reliability / observability | config.rs, theme.rs, launch.rs, lib.rs, terminal_backend.rs, updater.rs, session.rs | Pervasive `let _ =`, `.ok();`, `.ok()?`. Shell integration silently fails; PTY reader thread dies silently on `Err(_) => break`; Drop kills+waits silently | Users see "phantom" state; bad config silently reverts; PTY keystrokes vanish; updates fail invisibly | Convert each to `tracing::warn!/error!` with structured fields; surface critical paths in UI | |
| **REL-2** | P1 | Reliability | lib.rs, launch.rs | Five `App::new(...).expect("valid app")` sites | Pattern hides future failure modes (PTY/font) | Single fallible factory `build_app() -> anyhow::Result<App>`; propagate to `run()` | Yes (T5) |
| **OBS-1** | P1 | Observability | workspace-wide | `tracing` initialised in main.rs, but terminal-core / terminal-ansi / terminal-screen / terminal-pty contain **zero** `tracing::` calls; render-wgpu has no frame-time spans | Cannot diagnose frame stalls, PTY backpressure, atlas growth, slow config loads in production | `#[instrument]` on PTY pump, parser advance, render frame, atlas insert; metrics for `frame_us`, `pty_read_bytes`, `atlas_glyphs`, `pty_channel_depth` | |
| **REL-3** | P2 | Reliability | session.rs | `thread::spawn` without retaining JoinHandle; Drop kills child but never joins reader; `sync_channel(64)` with no metric | No visibility into reader-thread death; child can outlive parent | Retain handle, join with timeout in Drop, status channel back to App, channel-depth metric | |
| **DEP-1** | P1 | Dep hygiene | terminal-core/Cargo.toml `thiserror = "2"`; all others `"1"` | Two macro+impl trees in the binary | Bloat; can't `#[from]` cleanly across version boundary | Standardise on v2 via `[workspace.dependencies]` | Yes (T11) |
| **DEP-2** | P1 | Dep hygiene | Cargo.toml only `tracing*` hoisted; `winit = "0.29"` declared in 4 crates; `fontdb`, `serde`, `serde_json`, `anyhow`, `thiserror` duplicated | Drift risk; harder upgrades | Hoist all common deps; per-crate `{ workspace = true }` | Yes (T11) |
| **DEP-4** | P1 | Security/CI | ci.yml | fmt + clippy + test + doc only — no advisory or duplicate-version gate | New CVE or duplicate dep silently merged | Add `cargo-deny` job (advisories + licenses + bans) | |
| **MOD-1** | P2 | Maintainability | geometry.rs (~1,300 LOC, single file) | All `snapshot_to_*`, IME, scrollbar, unsafe cast in one module | Largest file in the repo; every render tweak rebuilds it; encourages copy-paste | Split into `geometry/{cell_quads,text_quads,scrollbar,ime,layout_ctx}.rs`; introduce `LayoutContext` | |
| **MOD-2** | P2 | Maintainability | lib.rs | `run()` closure inlines keyboard/mouse/paste/PTY pump/snapshot build | `cognitive_complexity` lint had to be allow-listed | Extract `on_keyboard/on_pointer/on_paste/on_pty_event/build_snapshot` with `EventCtx` | |
| **MOD-3** | P2 | Modularity | session.rs | Direct `use terminal_ansi::Parser` / `terminal_screen::Screen` | Blueprint promises swappable parser/screen; reality blocks fakes for tests | `trait TerminalParser`, `trait TerminalDisplay`; make session generic with defaults | |
| **MOD-4** | P2 | Modularity | window.rs (`objc2` NS* msg_send), coords.rs (`proc_pidinfo` FFI) | macOS FFI in render and CLI layers | `platform-abstraction` exists specifically for this; Windows/Linux port debt | Move titlebar/icon to `platform-abstraction::macos`, cwd lookup behind `ProcessInfo` trait | |
| **MOD-5** | P2 | Maintainability | config.rs, launch.rs, theme.rs | Three independent computations of XDG paths; color parsing duplicated | Drift between modules | One `paths::` module + one `color::parse_hex` | |
| **API-1** | P2 | API/perf | types.rs | `RenderSnapshot` flattens grid to `String` + parallel `Vec<Option<[f32;3]>>` + `Vec<u8>` styles; `DamageRegion` not tied in | Three parallel vectors → sync bugs; allocates a `String` every frame; row/col info lost; renderer can't skip clean cells | Replace with `Vec<RenderRow>` with embedded dirty bit + `Arc<DamageRegion>` | |
| **API-2** | P2 | Performance | terminal-screen/src/screen.rs, render-wgpu/src/types.rs | `DamageRegion { dirty_rows: Vec<(usize,usize)> }` — row-granular only | Cursor blink redraws entire row; atlas thrash in vim/htop | Cell-bitset damage with rect coalescing (matches blueprint §3.4) | |
| **PERF-1** | P2 | Performance | screen.rs | `*self.ansi_cache.borrow_mut() = Some((self.version, out.clone())); out` | Double-allocates on every cache miss | Cache `Arc<String>`; return `Arc::clone` | |
| **REL-4** | P2 | Performance/reliability | atlas.rs | `entries: HashMap<GlyphKey, GlyphEntry>` with no eviction | Long sessions cycling font sizes/emojis exhaust the atlas | LRU cap + repack on miss-rate threshold + metrics | |
| **TST-1** | P1 | Testability | render-wgpu (geometry: 0 tests; atlas: 0 tests); terminal-pty (3 mock-only tests; thread shutdown + shell integration untested); terminal-screen (7 tests; no wide-char resize / shrink); app-cli (theme/settings have no schema tests) | — | Largest/highest-risk modules have no safety net for refactors | `proptest` for `Screen::resize`; integration tests on `MockPty`; golden snapshot tests for geometry | |
| **TST-2** | P2 | Testability | no `tests/e2e_*` in app-cli | — | No regression net for the integration of the layers | Headless smoke booting App+MockPty, drive one frame, assert on `RenderSnapshot` | |
| **DOC-1** | P2 | Documentation | `EditorBuffer`, `GapBuffer`, `Cursor`, `Selection`, `SemanticCommand`, `LanguageHighlighter`, `TerminalSession`, `Action`, `Parser`, `Screen`, `ScreenSnapshot`, `DamageRegion`, `StyledChars`, `TabBackend`, `Renderer`, `GlyphAtlas::get_or_render_glyph` | Public items lack doc comments | CI's `cargo doc -D warnings` only catches broken links | `#![warn(missing_docs)]` on every `lib.rs` + minimal one-liner docs | |
| **DOC-2** | P3 | Documentation | repo memory blueprint vs. reality | ~75% alignment: gap buffer (not rope), shell-token highlighter (not tree-sitter), std::thread (not Tokio), flat platform layout (not unix/windows/macos) | New contributors follow stale design | Add `docs/ARCHITECTURE.md` describing as-built; archive blueprint | |
| **PERF-2** | P3 | Performance | launch.rs | Synchronous `fs::write` on shutdown path | Minor; visible only during quit | Detached thread with join-with-timeout | |
| **LINT-1** | P3 | Debt marker | Cargo.toml | `too_many_lines = "allow"`, `cognitive_complexity = "allow"` with comment "Phase 2 tasks T4/T14 will reduce these" | Acknowledged debt | Revert to `warn` after MOD-1/MOD-2 | |

### B. Refactor Tasks

Full per-task detail (steps, AC, tests, risks) lives in plan.md. Summary:

| ID | Title | Type | Pri | Effort | Covers | Is done |
|----|-------|------|-----|--------|--------|--------|
| **T1**  | Sign & verify auto-update artifacts (minisign + sha256 + rollback + UI surfacing) | security | P0 | M | SEC-1 | Yes |
| **T2**  | Validate untrusted session + config bounds; clamp font/padding/scrollback | security | P0 | S | SEC-2, REL-1 (config silence) | Yes |
| **T3**  | `unsafe_code = "forbid"` workspace-wide; replace geometry cast with `bytemuck` | security | P0 | S | SEC-3 | Yes |
| **T4**  | Replace `let _ =` / `.ok();` with `tracing` + PTY status surfacing in UI | reliability | P0 | M | REL-1, REL-3 | |
| **T5**  | Centralise fallible `build_app()` factory; remove 5 production `.expect()`s | reliability | P1 | S | REL-2 | Yes |
| **T6**  | Tracing spans + `metrics` crate baseline (PTY/parse/frame/atlas/config/update) | observability | P1 | M | OBS-1 | |
| **T7**  | `TerminalParser` / `TerminalDisplay` traits; generic `TerminalSession` | modularity | P2 | M | MOD-3 | |
| **T8**  | Split `geometry.rs` into module + `LayoutContext` + snapshot tests | maintainability | P2 | M | MOD-1, TST-1 (geom) | |
| **T9**  | Extract `app_cli::run` event handlers into named fns with `EventCtx` | maintainability | P2 | S | MOD-2 | |
| **T10** | Move macOS FFI + `proc_pidinfo` behind `platform-abstraction` | modularity | P2 | M | MOD-4 | |
| **T11** | Hoist common deps into `[workspace.dependencies]`; unify `thiserror` on v2 | dep hygiene | P1 | S | DEP-1, DEP-2, DEP-3, MOD-5 | Yes |
| **T12** | Add `cargo-deny` (advisories + licenses + bans) to CI | security/CI | P1 | S | DEP-4 | |
| **T13** | LICENSE + per-crate license metadata | legal | P0 | S | LIC-1 | Yes |
| **T14** | Property tests for `Screen::resize`; integration tests for PTY; golden snapshots; theme schema fuzz; E2E smoke | testability | P1 | M | TST-1, TST-2 | |
| **T15** | `#![warn(missing_docs)]` per crate; refresh blueprint as `docs/ARCHITECTURE.md` | documentation | P2 | S | DOC-1, DOC-2 | |
| **T16** | Redesign `RenderSnapshot` to `Vec<RenderRow>` + cell-bitset `DamageRegion`; `Arc<String>` cache | API/perf | P2 | L | API-1, API-2, PERF-1 | |
| **T17** | Glyph atlas LRU + repack + metrics | performance | P3 | M | REL-4 | |

**Suggested execution waves**
- **P0 (security/legal/correctness):** T13 → T1, T2, T3 → T4 → T5 [<< Current]
- **P1 (hygiene & observability):** T11 → T12 → T6 → T14
- **P2 (maintainability):** T9, T8, T7, T10, T15
- **P3 (performance/API redesign):** T16, T17

Each task entry in plan.md contains its own **Context / Steps / Acceptance Criteria / Suggested Tests / Risks & Dependencies**. Want me to expand any task into a ready-to-execute issue, or reorder priorities?