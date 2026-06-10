# Structured command execution blocks

## Decision summary

Teletipo should evolve its existing OSC 133 `CommandZone` lifecycle into a structured **execution block** lifecycle. The terminal grid remains the source of truth for terminal rendering, while each block adds semantic metadata and stable references to the prompt, command, and output already rendered in that grid.

This is deliberately not a second transcript model. A block is an enriched OSC 133 zone owned by `GenericTerminalSession`; `app-cli` only owns view state such as the selected block and which blocks are collapsed.

The first implementation should:

- create blocks only when OSC 133 shell integration is available;
- preserve the current plain terminal experience when integration is unavailable or markers are malformed;
- replace prompt navigation with block navigation while retaining the old command-palette names as aliases during migration;
- keep block metadata in memory only, just like the current command zones; and
- cap retained blocks consistently with scrollback rather than keeping the current independent 500-zone cap.

## Current implementation and constraints

The existing implementation already has most of the lifecycle needed for blocks:

- `terminal-pty` injects OSC 133 A/B/C/D and OSC 7 hooks for zsh and bash. The pre-command hook currently emits B and C consecutively, and the pre-prompt hook emits D, A, then OSC 7.
- `terminal-ansi` emits every non-hyperlink OSC payload as `Action::Osc(String)`.
- `terminal-core::GenericTerminalSession` interprets OSC 133, stores a `current_zone`, moves completed zones into `command_zones`, records absolute prompt/command/output rows, and records the exit code.
- `app-cli` keeps a separate `pending_cmd` and assumes `history[i]` corresponds to `command_zones()[i]`. That assumption is already acknowledged by the accessibility-tree builder and breaks for prompt-only zones, commands not submitted through the editor, malformed marker sequences, and zone eviction.
- Prompt navigation derives prompt rows from command zones and scrolls to an absolute row.
- The screen stores a primary grid plus scrollback and reflows visible primary-grid rows when its width changes. Plain absolute row numbers therefore are not sufficient long-term anchors for block boundaries.
- The accessibility tree already exposes completed command zones, but output is empty because there is no range-extraction API.

The feature should consolidate these parallel pieces rather than add another list in `TabState`.

## Goals and non-goals

### Goals for the first implementation

1. Represent the prompt, executed command, output, exit code, working directory, start time, and duration for each OSC 133 command lifecycle.
2. Make command/output copying independent of terminal text selection.
3. Support previous/next block navigation, exact re-run, load-for-edit, and collapse/expand.
4. Add lightweight visual status and duration without repainting command output into a separate widget.
5. Remain correct when commands produce no output, fail, use multiple lines, or complete while the tab is in the background.
6. Degrade safely when markers are missing, duplicated, or out of order.

### Non-goals for the first implementation

- Persisting blocks across application launches.
- Reconstructing blocks for restored legacy terminal text.
- Structuring alternate-screen application output (`vim`, `less`, full-screen TUIs).
- Parsing pipelines or shell syntax into subcommands.
- Re-running a command automatically in its original working directory. The original directory is displayed and retained, but re-run executes in the shell's **current** directory. A future explicit “Run in original directory” action can safely quote and prepend `cd`.
- Giving non-integrated shells heuristic blocks. They retain today's plain terminal behavior.

## Data model

### Terminal-owned semantic model

Replace/enrich `terminal_core::CommandZone` with `ExecutionBlock`. During migration, `command_zones()` and `prompt_marks()` can remain compatibility accessors over the new block collection.

```rust
pub struct ExecutionBlock {
    pub id: BlockId,
    pub phase: ExecutionPhase,

    // Stable screen references; not raw row indices.
    pub prompt_start: ScreenAnchor,
    pub command_end: Option<ScreenAnchor>,
    pub output_start: Option<ScreenAnchor>,
    pub output_end: Option<ScreenAnchor>,

    // Semantic values used by actions and accessibility.
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub cwd: Option<PathBuf>,
    pub started_at: Option<SystemTime>,
    pub duration: Option<Duration>,

    // Monotonic time is retained only while running so elapsed duration is
    // unaffected by wall-clock changes.
    started_mono: Option<Instant>,
}

pub enum ExecutionPhase {
    Prompt,
    Running,
    Output,
    Completed,
    Interrupted,
}
```

`BlockId` is a monotonically increasing session-local integer. UI state and accessibility nodes refer to IDs, never vector indices.

The prompt and output remain ranges in the screen. Plain text is extracted on demand through the screen model. This avoids duplicating potentially large output in both scrollback and every block. `command` is stored explicitly because it is required for exact copy/re-run/edit and cannot always be recovered reliably from rendered cells after wrapping or shell prompt transformations.

For commands submitted through the dedicated editor, `app-cli` registers the exact editor text with the terminal session before writing it to the PTY. For commands that reach the shell without that registration, B attempts a best-effort extraction from the prompt range; if extraction is ambiguous, `command` remains `None` and command-specific actions are disabled.

`cwd` is a snapshot of the most recent valid OSC 7 path at B. It must not be a reference to the tab's mutable current directory. `started_at` is wall-clock time captured at B for display/accessibility, while duration is measured from a monotonic `Instant` and finalized at D.

Prompt-only lifecycles are retained only while current. If a later A arrives before B, the old prompt is discarded rather than inserted as a fake command block.

### Screen anchors and extraction

Add a small semantic-anchor facility to `terminal-screen`:

```rust
pub struct ScreenAnchor { /* opaque */ }

pub trait TerminalDisplay {
    fn cursor_anchor(&self) -> ScreenAnchor;
    fn text_between(&self, start: ScreenAnchor, end: ScreenAnchor) -> Option<String>;
    fn rows_between(&self, start: ScreenAnchor, end: ScreenAnchor) -> Option<RowRange>;
}
```

An anchor identifies a boundary in the logical terminal text stream, not merely `scrollback_len + cursor_row`. The screen updates anchors when visible rows reflow, rows move between the grid and scrollback, or old scrollback is evicted. If the beginning of a range is evicted, extraction returns the retained suffix and marks the block as partially evicted; if the whole range is gone, actions that require that text are disabled.

This is the only necessary foundational screen-model change. It benefits prompt navigation, output copying, collapse projection, and accessibility together.

### App/UI view state

Add only presentation state to `TabState`:

```rust
pub struct BlockViewState {
    pub selected: Option<BlockId>,
    pub collapsed: HashSet<BlockId>,
}
```

Collapse is not part of `ExecutionBlock`: it is a user-interface preference, not command execution history. Remove `pending_cmd` after editor submissions are registered directly with the current block. Shared command history remains a separate persisted feature; it should be updated from a completed block's explicit command instead of zipped by index with zones.

A collapsed block still retains and receives all terminal output. The renderer projects its output range to a one-row placeholder; expanding it restores the normal terminal rows without replaying the PTY stream.

## State transitions

OSC 133 markers remain the lifecycle authority. Unexpected transitions are handled defensively and never panic.

| Current state | Event | Next state | Actions |
|---|---|---|---|
| none | `133;A` | Prompt | Open block candidate at cursor anchor. Do not assign an ID visible to UI yet. |
| Prompt | editor submission | Prompt | Attach exact command text to the candidate before PTY write. |
| Prompt | `133;B` | Running | Assign/confirm ID, set command-end anchor, snapshot OSC 7 CWD, capture `SystemTime` and `Instant`. |
| Running | `133;C` | Output | Set output-start anchor. B and C may share an anchor. |
| Output or Running | `133;D;<code>` | Completed | Set output-end anchor, exit code, and monotonic duration; publish completion event; commit explicit command to shared history. If C was missing, use command-end as output-start. |
| Prompt | next `133;A` | Prompt | Discard the old prompt-only candidate and open a new one. |
| Running or Output | next `133;A` | Interrupted, then Prompt | Close old block at current cursor with no exit code/duration if D was missing, then open new candidate. |
| any | duplicate marker | unchanged | Ignore if it would move backward or overwrite a set boundary. |
| any | scrollback eviction | unchanged | Mark affected ranges partial/unavailable; keep metadata and command actions. |

A D received without an active running/output block still updates the backwards-compatible “last exit code” signal but does not invent a block. This preserves current runtime behavior while avoiding misleading UI.

Alternate-screen entry does not end the block. Anchors and extraction refer to the primary screen; status/duration appear after the shell returns and emits D.

### UI action transitions

| Action | Preconditions | Effect |
|---|---|---|
| Select previous/next block | At least one published block | Select by `BlockId`, scroll its prompt/header into view, and clear ordinary terminal selection. |
| Copy command | Selected block has `command` | Copy exact command text, including embedded newlines; do not add a trailing newline. |
| Copy output | Selected block has a retained output range | Copy plain output text only, trim right-padding cells per visual row, preserve meaningful newlines, and omit prompt/command/header. |
| Re-run | Selected completed/interrupted block has `command`, no foreground child is active | Submit exact command through the existing editor execution path in the shell's current CWD. Preserve unrelated editor text by requiring the editor to be empty; otherwise show a confirmation/toast and offer Edit instead. |
| Edit | Selected block has `command` | Replace editor contents with the command, focus editor, place cursor at end, and do not submit. |
| Collapse/expand | Selected block has output over the threshold | Toggle its ID in `collapsed`; keep selected header visible. |

## Required parser and terminal-session changes

### `terminal-ansi`

Keep the existing OSC byte framing, but stop making every consumer parse OSC 133 strings independently. Add a typed action while preserving `Action::Osc(String)` for unsupported/general OSC payloads:

```rust
pub enum ShellIntegration {
    PromptStart,             // A
    CommandStart,            // B
    OutputStart,             // C
    CommandFinished(i32),    // D;code
    WorkingDirectory(PathBuf), // parsed/validated OSC 7, optional in first patch
}

Action::ShellIntegration(ShellIntegration)
```

The parser should recognize BEL- and ST-terminated OSC 133 forms, accept only an entire signed 32-bit exit code, and leave malformed/extended forms as generic OSC actions. OSC 7 may be typed in the same patch or continue through the current path initially; it must be validated before becoming a CWD snapshot.

This is a narrow refinement of existing OSC support, not a general semantic-command protocol.

### `terminal-core`

- Replace `current_zone`/`command_zones` with `current_block`/`execution_blocks`.
- Add `register_submitted_command(String)` so the dedicated editor attaches exact text before PTY input.
- Process typed shell-integration actions through one transition function.
- Capture anchors from `TerminalDisplay` at A/B/C/D.
- Expose block lookup by `BlockId`, ordered IDs, text extraction helpers, and completion events.
- Preserve `take_last_exit_code`, `prompt_marks`, and `command_zones` temporarily as adapters so the change can land in stages.
- Prune blocks only when their screen ranges are fully evicted and they are beyond a small metadata-only retention limit. Never prune by an index that UI state or history assumes is stable.

No additional shell marker is required for v1. The current consecutive B/C markers are sufficient; their timestamps/ranges may be identical for commands with no separately observable pre-output interval.

### `terminal-pty`

Keep the current zsh/bash injection. Only clarify comments and add integration fixtures that assert marker ordering:

1. initial D/A/OSC 7 prompt sequence;
2. B/C before command output;
3. D/A/OSC 7 after completion.

Changing marker payloads would make the first implementation less compatible and is unnecessary because exact editor commands are registered in-process.

## Required screen-model changes

1. **Stable anchors:** implement opaque logical-text anchors and update them through scroll, resize/reflow, clear, and eviction.
2. **Range extraction:** provide plain-text extraction between anchors. It must understand wrapped rows, wide characters, and right-padding cells.
3. **Row projection:** expose the current visual row range for an anchor range so navigation and rendering do not duplicate coordinate math.
4. **Collapsed projection:** allow the renderer-facing dump/styled-row APIs to skip selected output ranges and insert a synthetic placeholder row. The underlying primary grid and scrollback do not change.
5. **Damage/versioning:** toggling collapse or a selected block header must trigger a full redraw/projection-version change without pretending terminal cells changed.

Keep collapse projection outside ANSI parsing and cell mutation. Programs must continue to observe normal terminal dimensions, and expanding a block must be lossless.

## Required UI changes

### Block presentation

Render a subtle one-row block header aligned with the prompt start. It should not look like a card around every command.

- Left edge: a 2-pixel or narrow-cell status accent: neutral/running, muted green for exit 0, muted red for non-zero, gray for interrupted/unknown.
- Header text: command when available, then compact metadata such as `✓ 0  842 ms` or `✕ 127  2.4 s`. Do not rely on color alone; include a small glyph and expose an accessible status label.
- Optional secondary tooltip/context-menu detail: original working directory and absolute start time.
- Running block: subtle neutral indicator and live elapsed duration, refreshed at a low frequency (for example 4 Hz) without invalidating terminal cells.
- Long output: after a default threshold such as 20 visual rows, show a collapse control. A collapsed placeholder says, for example, `128 output lines · 2.4 s · expand`.
- Selected block: faint background/accent on the header only. Normal text selection remains visually distinct.

For a command with no output, render only the header/status; do not insert an empty output row.

### Actions and command palette

Add `CommandId` variants for:

- Select Previous Command Block
- Select Next Command Block
- Copy Command from Selected Block
- Copy Output from Selected Block
- Re-run Selected Command
- Edit Selected Command
- Collapse/Expand Selected Block

Keep “Jump to Previous Prompt” and “Jump to Next Prompt” as palette aliases for one release, routed to block selection. Add the same actions to the terminal context menu when a block is selected or right-clicked. Right-click should select the block under the pointer before opening the menu.

“Edit” must use an editor-core replace-all operation so it participates cleanly in editor undo history. “Re-run” must pass through the existing `run_editor_command`/PTY submission path so history, shell integration, and multiline handling stay consistent.

### Accessibility

Populate the existing command-zone accessibility node from `ExecutionBlock`, including actual output text when retained, CWD, duration, and status. Selected/collapsed state should be exposed. Completion announcements should use block data rather than `history.zip(command_zones)`.

## Keyboard shortcuts

Shortcuts are active only while terminal/block focus is active; when no block is selected, action shortcuts do nothing and show a brief “Select a command block first” toast. All actions also remain available from the command palette for discoverability and accessibility.

| Shortcut | Action | Rationale |
|---|---|---|
| `Cmd+Shift+↑/↓` (macOS), `Ctrl+Shift+↑/↓` (Linux/Windows) | Previous/next command block | Extends current prompt navigation without consuming plain arrows. |
| `Alt+Shift+C` | Copy selected command | Avoids conflicting with terminal-selection copy. |
| `Alt+Shift+O` | Copy selected output | Mnemonic and distinct from command copy. |
| `Alt+Shift+R` | Re-run selected command | Explicit enough to avoid accidental Enter-based execution. |
| `Alt+Shift+E` | Edit selected command | Loads but never executes. |
| `Alt+Shift+Space` | Collapse/expand selected block | Mirrors disclosure toggling without taking plain Space from the shell. |
| `Escape` | Clear block selection | Returns to ordinary terminal interaction. |

Before implementation, platform smoke tests must verify that these combinations are not already reserved by the window manager. If `Alt+Shift` proves unreliable on a target, keep the palette/context-menu actions and use configurable shortcuts later rather than intercepting common shell keys.

## Staged implementation plan

### Stage 1 — Make OSC 133 lifecycle authoritative

- Add typed OSC 133 parser actions and parser tests.
- Enrich/rename `CommandZone` to `ExecutionBlock` with ID, phase, CWD snapshot, start time, duration, and explicit command.
- Add `register_submitted_command`; remove `TabState::pending_cmd` and index-zipping with history.
- Route history finalization and accessibility completion data from block completion events.
- Keep current rendering and prompt navigation through compatibility accessors.

**Deliverable:** correct structured metadata and duration with no visual behavior change.

### Stage 2 — Stable screen ranges and copy actions

- Add screen anchors and range extraction.
- Migrate block boundaries and prompt navigation from absolute rows to anchors.
- Populate output accessibility text.
- Add block selection, previous/next navigation, copy command, and copy output actions.

**Deliverable:** reliable semantic navigation and copying across ordinary scrolling/resizing.

### Stage 3 — Re-run, edit, and status UI

- Add selected-block header styling, success/failure/running indicator, and duration.
- Add command palette, keyboard, and context-menu actions.
- Implement re-run through the existing submission path and edit through editor-core replace-all.
- Add completion announcements based on block data.

**Deliverable:** all required actions except collapse.

### Stage 4 — Collapse projection and hardening

- Add collapsed row projection without mutating terminal cells.
- Add placeholder rendering, hit testing, scrollbar accounting, selection/search behavior, and accessibility state.
- Harden eviction, reflow, huge output, alternate-screen, malformed marker, and background-tab behavior.

**Deliverable:** complete first implementation.

## Test plan

### Unit tests

#### `terminal-ansi`

- Parse A/B/C and `D;0`, `D;1`, negative and maximum/minimum 32-bit exit codes with BEL and ST terminators.
- Leave malformed D codes, overflow, missing code, and unknown OSC 133 subcommands as generic OSC.
- Verify arbitrary OSC and OSC 8 behavior is unchanged.

#### `terminal-core`

- A → B → C → D produces one completed block with command, exit code, CWD snapshot, start time, and non-negative duration.
- Consecutive B/C creates valid equal boundaries.
- A prompt with no B is discarded when the next A arrives.
- Missing C falls back to command-end for output start.
- Missing D followed by A produces an interrupted block.
- Duplicate/out-of-order markers do not regress state or duplicate blocks.
- `register_submitted_command` binds to the active prompt candidate and supports multiline commands.
- A later OSC 7 update does not mutate a completed block's CWD.
- D without a running block updates only the compatibility exit-code signal.
- Completion events update history from the block command without relying on vector indices.

#### `terminal-screen`

- Anchors survive linefeed/scrollback movement and height changes.
- Anchors and extracted text remain correct after width reflow, including wrapped and wide-character lines.
- Range extraction excludes right-padding cells and preserves meaningful line breaks.
- Partial and full scrollback eviction return the documented availability state.
- Collapse projection hides only output rows, inserts one placeholder, preserves underlying text, and is lossless after expand.
- Projection updates row counts, hit testing, damage, and scroll offsets consistently.

#### `app-cli`

- Previous/next navigation selects by ID and scrolls the header into view.
- Copy command/output places only the requested text on the clipboard.
- Re-run is disabled while a foreground child is active and uses the current CWD.
- Re-run does not overwrite non-empty editor text without confirmation.
- Edit replaces editor contents, focuses it, and remains undoable.
- Collapse is offered only above the output threshold and survives selection changes.
- Exit 0, non-zero, running, and interrupted states select the correct non-color-only indicator.

### Integration tests

Use a deterministic fake PTY for most tests and real zsh/bash smoke tests where available.

1. **Happy path:** emit prompt, run `printf 'hello\n'`, and assert one block with exact command, output `hello`, exit 0, CWD, and duration.
2. **Failure:** run a command that exits 7 and assert failure indicator, copy output, and accessibility status.
3. **No output:** run `true` and assert a valid block with empty output and no phantom row.
4. **Multiline command:** submit through the dedicated editor, then verify exact copy/edit/re-run text.
5. **Long output:** produce more than the collapse threshold, collapse/expand, and assert following blocks remain navigable and output is unchanged.
6. **Resize and scrollback:** complete several blocks, resize narrower/wider, scroll, and verify navigation/copy still target the correct block.
7. **Eviction:** exceed the scrollback limit and verify unavailable output actions disable gracefully while command metadata remains usable.
8. **Background tab:** complete a command in an inactive tab and verify duration/status when activated.
9. **Alternate screen:** run a fake alternate-screen sequence inside a block and verify primary-screen block completion remains valid.
10. **Malformed markers/no integration:** verify Teletipo remains a normal terminal, does not invent blocks, and retains existing history behavior.
11. **Real shell hooks:** in CI environments with zsh/bash, assert injected marker order and OSC 7 CWD snapshots around a command.
12. **Accessibility:** assert a completed block node contains command, retained output, exit status, duration, CWD, and collapsed/selected state.

### Performance checks

- Benchmark feeding output into a running block versus current `TerminalSession::feed`; semantic tracking should remain O(1) per marker, not per printed character.
- Benchmark extracting/copying a large block only when requested.
- Benchmark collapsed projection and rendering with hundreds of blocks.
- Assert retained metadata is bounded and large output is not duplicated per block.

## Acceptance criteria

The first implementation is complete when an integrated zsh or bash session can execute commands normally and every completed command can be selected, navigated, copied by command/output, re-run, loaded into the editor, and collapsed/expanded; its status, CWD, start time, and duration are available semantically; resizing and scrollback do not silently retarget actions; and non-integrated shells continue to behave as they do today.
