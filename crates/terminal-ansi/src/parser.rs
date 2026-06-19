use crate::action::Action;

/// Returns the expected total byte length of a UTF-8 sequence given its lead byte.
/// Returns 0 for invalid lead bytes.
fn utf8_seq_len(lead: u8) -> u8 {
    match lead {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ParserState {
    #[default]
    Ground,
    Escape,
    /// ESC followed by an intermediate byte (e.g. `(`, `)`, `*`, `+`).
    /// Consumes exactly one more byte (the charset designator) and returns to Ground.
    EscIntermediate,
    Csi,
    Osc,
}

/// Incremental ANSI / VT escape-sequence parser.
///
/// Feed it raw PTY bytes via [`Parser::advance`] to produce a vector of
/// [`crate::Action`] events.  The parser keeps internal state across calls so
/// sequences split across reads are handled correctly.
#[derive(Debug, Default)]
pub struct Parser {
    state: ParserState,
    csi_buf: Vec<u8>,
    osc_buf: Vec<u8>,
    osc_esc_seen: bool,
    /// Partial UTF-8 sequence accumulator (up to 4 bytes).
    utf8_buf: [u8; 4],
    /// Number of bytes currently in `utf8_buf`.
    utf8_len: u8,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            state: ParserState::Ground,
            csi_buf: Vec::with_capacity(64),
            osc_buf: Vec::with_capacity(64),
            osc_esc_seen: false,
            utf8_buf: [0u8; 4],
            utf8_len: 0,
        }
    }

    pub fn advance(&mut self, bytes: &[u8]) -> Vec<Action> {
        let mut actions = Vec::with_capacity(bytes.len());
        for byte in bytes {
            self.feed_byte(*byte, &mut actions);
        }
        actions
    }

    #[allow(clippy::too_many_lines)]
    fn feed_byte(&mut self, byte: u8, actions: &mut Vec<Action>) {
        match self.state {
            ParserState::Ground => match byte {
                0x1b => {
                    self.utf8_len = 0; // discard any incomplete sequence
                    self.state = ParserState::Escape;
                }
                b'\n' => actions.push(Action::Linefeed),
                b'\r' => actions.push(Action::CarriageReturn),
                0x07 => actions.push(Action::Bell),
                0x08 => actions.push(Action::Backspace),
                0x09 => actions.push(Action::HorizontalTab),
                0x20..=0x7e => actions.push(Action::Print(byte as char)),
                // Multi-byte UTF-8 lead byte: start a new sequence.
                0xc0..=0xf7 => {
                    self.utf8_buf[0] = byte;
                    self.utf8_len = 1;
                }
                // UTF-8 continuation byte.
                0x80..=0xbf => {
                    if self.utf8_len > 0 && self.utf8_len < 4 {
                        let idx = self.utf8_len as usize;
                        self.utf8_buf[idx] = byte;
                        self.utf8_len += 1;
                        let expected = utf8_seq_len(self.utf8_buf[0]);
                        if self.utf8_len == expected {
                            let slice = &self.utf8_buf[..expected as usize];
                            if let Ok(s) = std::str::from_utf8(slice)
                                && let Some(ch) = s.chars().next()
                            {
                                actions.push(Action::Print(ch));
                            }
                            self.utf8_len = 0;
                        }
                    } else {
                        self.utf8_len = 0; // stray continuation, reset
                    }
                }
                _ => {}
            },
            ParserState::Escape => match byte {
                b'[' => {
                    self.csi_buf.clear();
                    self.state = ParserState::Csi;
                }
                b']' => {
                    self.osc_esc_seen = false;
                    self.osc_buf.clear();
                    self.state = ParserState::Osc;
                }
                b'7' => {
                    actions.push(Action::SaveCursor);
                    self.state = ParserState::Ground;
                }
                b'8' => {
                    actions.push(Action::RestoreCursor);
                    self.state = ParserState::Ground;
                }
                // ESC M — reverse index (scroll up / RI)
                b'M' => {
                    actions.push(Action::ReverseIndex);
                    self.state = ParserState::Ground;
                }
                // ESC D — index (IND, same as linefeed)
                b'D' => {
                    actions.push(Action::Linefeed);
                    self.state = ParserState::Ground;
                }
                // ESC E — next line (NEL)
                b'E' => {
                    actions.push(Action::CarriageReturn);
                    actions.push(Action::Linefeed);
                    self.state = ParserState::Ground;
                }
                // ESC c — full reset (RIS) — treat as clear screen + home
                b'c' => {
                    actions.push(Action::EraseInDisplay(2));
                    actions.push(Action::CursorPosition { row: 1, col: 1 });
                    self.state = ParserState::Ground;
                }
                // ESC = / ESC > — application/normal keypad mode (ignored)
                b'=' | b'>' => {
                    self.state = ParserState::Ground;
                }
                // ESC ( ESC ) ESC * ESC + — charset designation sequences.
                // The following byte is the designator (e.g. 'B' for ASCII,
                // '0' for DEC special graphics). We don't implement character
                // set switching but must consume the extra byte so it is not
                // printed as a literal character.
                b'(' | b')' | b'*' | b'+' => {
                    self.state = ParserState::EscIntermediate;
                }
                _ => {
                    self.state = ParserState::Ground;
                }
            },
            ParserState::EscIntermediate => {
                // Consume the charset designator byte and return to Ground.
                self.state = ParserState::Ground;
            }
            ParserState::Csi => {
                if (0x40..=0x7e).contains(&byte) {
                    self.handle_csi_final(byte, actions);
                    self.csi_buf.clear();
                    self.state = ParserState::Ground;
                } else {
                    self.csi_buf.push(byte);
                }
            }
            ParserState::Osc => {
                if byte == 0x07 || (self.osc_esc_seen && byte == b'\\') {
                    let payload = String::from_utf8_lossy(&self.osc_buf).into_owned();
                    let action = parse_osc_payload(&payload);
                    actions.push(action);
                    self.osc_buf.clear();
                    self.osc_esc_seen = false;
                    self.state = ParserState::Ground;
                } else if byte == 0x1b {
                    self.osc_esc_seen = true;
                } else {
                    self.osc_esc_seen = false;
                    self.osc_buf.push(byte);
                }
            }
        }
    }

    fn handle_csi_final(&self, final_byte: u8, actions: &mut Vec<Action>) {
        let first = self.csi_buf.first().copied();
        let has_private_prefix = first == Some(b'?');
        let has_equal_prefix = first == Some(b'=');
        let param_slice = if has_private_prefix || has_equal_prefix {
            &self.csi_buf[1..]
        } else {
            self.csi_buf.as_slice()
        };

        let params = parse_params(param_slice);

        match final_byte {
            b'A' => actions.push(Action::CursorUp(first_or(&params, 1))),
            b'B' => actions.push(Action::CursorDown(first_or(&params, 1))),
            b'C' => actions.push(Action::CursorForward(first_or(&params, 1))),
            b'D' => actions.push(Action::CursorBackward(first_or(&params, 1))),
            b'E' => actions.push(Action::CursorNextLine(first_or(&params, 1))),
            b'F' => actions.push(Action::CursorPreviousLine(first_or(&params, 1))),
            b'G' | b'`' => actions.push(Action::CursorHorizontalAbsolute(first_or(&params, 1))),
            b'd' => actions.push(Action::CursorVerticalAbsolute(first_or(&params, 1))),
            b'H' | b'f' => {
                let row = first_or(&params, 1);
                let col = nth_or(&params, 1, 1);
                actions.push(Action::CursorPosition { row, col });
            }
            b's' => actions.push(Action::SaveCursor),
            b'u' => {
                if has_private_prefix {
                    // \x1b[?u — query current kitty keyboard flags
                    actions.push(Action::KittyKeyboardQuery);
                } else if has_equal_prefix {
                    // \x1b[=<flags>u — push flags onto kitty stack
                    actions.push(Action::KittyKeyboardPush(u32::from(first_or(&params, 0))));
                } else if !params.is_empty() && params[0] > 0 {
                    // \x1b[<n>u — pop n entries from kitty stack
                    actions.push(Action::KittyKeyboardPop(u32::from(params[0])));
                } else {
                    // \x1b[u (plain) — VT220 restore cursor
                    actions.push(Action::RestoreCursor);
                }
            }
            b'r' => actions.push(Action::SetScrollRegion {
                top: first_or(&params, 1),
                bottom: nth_or(&params, 1, 0),
            }),
            b'@' => actions.push(Action::InsertChars(first_or(&params, 1))),
            b'P' => actions.push(Action::DeleteChars(first_or(&params, 1))),
            b'L' => actions.push(Action::InsertLines(first_or(&params, 1))),
            b'M' => actions.push(Action::DeleteLines(first_or(&params, 1))),
            b'J' => actions.push(Action::EraseInDisplay(first_or(&params, 0))),
            b'K' => actions.push(Action::EraseInLine(first_or(&params, 0))),
            b'm' => {
                let sgr = if params.is_empty() { vec![0] } else { params };
                actions.push(Action::SetGraphicsRendition(sgr));
            }
            b'h' if has_private_prefix => {
                if let Some(mode) = params.first() {
                    actions.push(Action::DecPrivateModeSet(*mode));
                }
            }
            b'l' if has_private_prefix => {
                if let Some(mode) = params.first() {
                    actions.push(Action::DecPrivateModeReset(*mode));
                }
            }
            b'n' if !has_private_prefix && params.first().copied() == Some(6) => {
                actions.push(Action::DeviceStatusReport);
            }
            b'q' if !has_private_prefix => {
                // DECSCUSR: \x1b[N q — the space before 'q' is an intermediate byte.
                // Strip all intermediate bytes (0x20-0x2F) so parse_params sees just the digit.
                let numeric: Vec<u8> = param_slice
                    .iter()
                    .copied()
                    .filter(|b| !matches!(b, 0x20..=0x2F))
                    .collect();
                let shape_params = parse_params(&numeric);
                actions.push(Action::SetCursorShape(first_or(&shape_params, 0)));
            }
            _ => {}
        }
    }
}

/// Classify a raw OSC payload string into the most specific `Action` variant
/// available. Falls through to the generic `Osc(payload)` for unknown codes.
fn parse_osc_payload(payload: &str) -> Action {
    // OSC 8 — hyperlink: `8;[params];[uri]`
    // The params field is optional application-specific metadata; we skip it.
    if let Some(rest) = payload.strip_prefix("8;") {
        // rest is "[params];[uri]"
        if let Some(semi) = rest.find(';') {
            let uri = &rest[semi + 1..];
            if uri.is_empty() {
                return Action::SetHyperlink(None);
            } else {
                return Action::SetHyperlink(Some(uri.to_owned()));
            }
        }
    }
    Action::Osc(payload.to_owned())
}

fn parse_params(bytes: &[u8]) -> Vec<u16> {
    if bytes.is_empty() {
        return Vec::new();
    }

    bytes
        .split(|b| *b == b';')
        .map(|part| {
            if part.is_empty() {
                0
            } else {
                std::str::from_utf8(part)
                    .ok()
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(0)
            }
        })
        .collect()
}

fn first_or(params: &[u16], default: u16) -> u16 {
    params
        .first()
        .copied()
        .filter(|v| *v != 0)
        .unwrap_or(default)
}

fn nth_or(params: &[u16], idx: usize, default: u16) -> u16 {
    params
        .get(idx)
        .copied()
        .filter(|v| *v != 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::{Action, Parser};

    #[derive(Debug)]
    struct Fixture {
        name: &'static str,
        input: Vec<u8>,
        expected: Vec<Action>,
    }

    #[allow(clippy::too_many_lines)] // long-but-flat fixture table
    fn fixture_matrix() -> Vec<Fixture> {
        let mut out = Vec::new();

        out.push(Fixture {
            name: "plain_text",
            input: b"abc".to_vec(),
            expected: vec![Action::Print('a'), Action::Print('b'), Action::Print('c')],
        });
        out.push(Fixture {
            name: "controls",
            input: b"\n\r\x08\t".to_vec(),
            expected: vec![
                Action::Linefeed,
                Action::CarriageReturn,
                Action::Backspace,
                Action::HorizontalTab,
            ],
        });

        for n in 1..=10u16 {
            out.push(Fixture {
                name: "cursor_up",
                input: format!("\x1b[{}A", n).into_bytes(),
                expected: vec![Action::CursorUp(n)],
            });
            out.push(Fixture {
                name: "cursor_down",
                input: format!("\x1b[{}B", n).into_bytes(),
                expected: vec![Action::CursorDown(n)],
            });
            out.push(Fixture {
                name: "cursor_forward",
                input: format!("\x1b[{}C", n).into_bytes(),
                expected: vec![Action::CursorForward(n)],
            });
            out.push(Fixture {
                name: "cursor_backward",
                input: format!("\x1b[{}D", n).into_bytes(),
                expected: vec![Action::CursorBackward(n)],
            });
            out.push(Fixture {
                name: "cursor_next_line",
                input: format!("\x1b[{}E", n).into_bytes(),
                expected: vec![Action::CursorNextLine(n)],
            });
            out.push(Fixture {
                name: "cursor_previous_line",
                input: format!("\x1b[{}F", n).into_bytes(),
                expected: vec![Action::CursorPreviousLine(n)],
            });
            out.push(Fixture {
                name: "cursor_horizontal_absolute",
                input: format!("\x1b[{}G", n).into_bytes(),
                expected: vec![Action::CursorHorizontalAbsolute(n)],
            });
            out.push(Fixture {
                name: "horizontal_position_absolute",
                input: format!("\x1b[{}\x60", n).into_bytes(),
                expected: vec![Action::CursorHorizontalAbsolute(n)],
            });
            out.push(Fixture {
                name: "cursor_vertical_absolute",
                input: format!("\x1b[{}d", n).into_bytes(),
                expected: vec![Action::CursorVerticalAbsolute(n)],
            });
        }

        for n in 0..=3u16 {
            out.push(Fixture {
                name: "erase_in_display",
                input: format!("\x1b[{}J", n).into_bytes(),
                expected: vec![Action::EraseInDisplay(n)],
            });
            out.push(Fixture {
                name: "erase_in_line",
                input: format!("\x1b[{}K", n).into_bytes(),
                expected: vec![Action::EraseInLine(n)],
            });
        }

        for (row, col) in &[(1, 1), (2, 5), (10, 20), (24, 80), (40, 120)] {
            out.push(Fixture {
                name: "cursor_position",
                input: format!("\x1b[{};{}H", row, col).into_bytes(),
                expected: vec![Action::CursorPosition {
                    row: *row,
                    col: *col,
                }],
            });
        }

        for mode in &[1049u16, 1000u16, 25u16, 2004u16] {
            out.push(Fixture {
                name: "dec_private_set",
                input: format!("\x1b[?{}h", mode).into_bytes(),
                expected: vec![Action::DecPrivateModeSet(*mode)],
            });
            out.push(Fixture {
                name: "dec_private_reset",
                input: format!("\x1b[?{}l", mode).into_bytes(),
                expected: vec![Action::DecPrivateModeReset(*mode)],
            });
        }

        for sgr in &[
            vec![0u16],
            vec![1u16],
            vec![31u16],
            vec![1u16, 34u16],
            vec![0u16, 39u16, 49u16],
        ] {
            let payload = sgr
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(";");
            out.push(Fixture {
                name: "sgr",
                input: format!("\x1b[{}m", payload).into_bytes(),
                expected: vec![Action::SetGraphicsRendition(sgr.clone())],
            });
        }

        out.push(Fixture {
            name: "save_restore_esc",
            input: b"\x1b7\x1b8".to_vec(),
            expected: vec![Action::SaveCursor, Action::RestoreCursor],
        });
        out.push(Fixture {
            name: "save_restore_csi",
            input: b"\x1b[s\x1b[u".to_vec(),
            expected: vec![Action::SaveCursor, Action::RestoreCursor],
        });
        out.push(Fixture {
            name: "scroll_region",
            input: b"\x1b[2;12r".to_vec(),
            expected: vec![Action::SetScrollRegion { top: 2, bottom: 12 }],
        });

        for n in 1..=5u16 {
            out.push(Fixture {
                name: "insert_chars",
                input: format!("\x1b[{}@", n).into_bytes(),
                expected: vec![Action::InsertChars(n)],
            });
            out.push(Fixture {
                name: "delete_chars",
                input: format!("\x1b[{}P", n).into_bytes(),
                expected: vec![Action::DeleteChars(n)],
            });
            out.push(Fixture {
                name: "insert_lines",
                input: format!("\x1b[{}L", n).into_bytes(),
                expected: vec![Action::InsertLines(n)],
            });
            out.push(Fixture {
                name: "delete_lines",
                input: format!("\x1b[{}M", n).into_bytes(),
                expected: vec![Action::DeleteLines(n)],
            });
        }

        out
    }

    #[test]
    fn parses_ascii_and_controls() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"ab\n\r\x08");
        assert_eq!(
            actions,
            vec![
                Action::Print('a'),
                Action::Print('b'),
                Action::Linefeed,
                Action::CarriageReturn,
                Action::Backspace,
            ]
        );
    }

    #[test]
    fn parses_basic_csi_sequences() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b[2J\x1b[10;20H\x1b[31m");

        assert_eq!(
            actions,
            vec![
                Action::EraseInDisplay(2),
                Action::CursorPosition { row: 10, col: 20 },
                Action::SetGraphicsRendition(vec![31]),
            ]
        );
    }

    #[test]
    fn parses_dec_private_mode_for_alt_buffer() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b[?1049h\x1b[?1049l");

        assert_eq!(
            actions,
            vec![
                Action::DecPrivateModeSet(1049),
                Action::DecPrivateModeReset(1049)
            ]
        );
    }

    #[test]
    fn parses_save_restore_and_scroll_region() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b7\x1b8\x1b[s\x1b[u\x1b[2;20r");

        assert_eq!(
            actions,
            vec![
                Action::SaveCursor,
                Action::RestoreCursor,
                Action::SaveCursor,
                Action::RestoreCursor,
                Action::SetScrollRegion { top: 2, bottom: 20 },
            ]
        );
    }

    #[test]
    fn parses_insert_delete_sequences() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b[3@\x1b[2P\x1b[4L\x1b[5M");

        assert_eq!(
            actions,
            vec![
                Action::InsertChars(3),
                Action::DeleteChars(2),
                Action::InsertLines(4),
                Action::DeleteLines(5),
            ]
        );
    }

    #[test]
    fn runs_fixture_matrix() {
        let fixtures = fixture_matrix();
        assert!(
            fixtures.len() >= 50,
            "fixture matrix too small: {}",
            fixtures.len()
        );

        for fixture in fixtures {
            let mut parser = Parser::new();
            let actions = parser.advance(&fixture.input);
            assert_eq!(
                actions, fixture.expected,
                "fixture failed: {}",
                fixture.name
            );
        }
    }

    #[test]
    fn handles_split_sequences_across_chunks() {
        let mut parser = Parser::new();
        let a1 = parser.advance(b"\x1b[");
        let a2 = parser.advance(b"31mX");

        assert!(a1.is_empty());
        assert_eq!(
            a2,
            vec![Action::SetGraphicsRendition(vec![31]), Action::Print('X')]
        );
    }

    #[test]
    fn parses_two_byte_utf8() {
        // U+00E9 LATIN SMALL LETTER E WITH ACUTE — UTF-8: 0xC3 0xA9
        let mut parser = Parser::new();
        let actions = parser.advance("\u{00e9}".as_bytes());
        assert_eq!(actions, vec![Action::Print('\u{00e9}')]);
    }

    #[test]
    fn parses_three_byte_utf8_nerd_font_icon() {
        // U+E0B0 (Nerd Font powerline arrow) — UTF-8: 0xEE 0x82 0xB0
        let bytes: &[u8] = &[0xEE, 0x82, 0xB0];
        let mut parser = Parser::new();
        let actions = parser.advance(bytes);
        assert_eq!(actions, vec![Action::Print('\u{e0b0}')]);
    }

    #[test]
    fn parses_four_byte_utf8_emoji() {
        // U+1F600 GRINNING FACE — UTF-8: 0xF0 0x9F 0x98 0x80
        let bytes: &[u8] = &[0xF0, 0x9F, 0x98, 0x80];
        let mut parser = Parser::new();
        let actions = parser.advance(bytes);
        assert_eq!(actions, vec![Action::Print('\u{1f600}')]);
    }

    #[test]
    fn parses_utf8_split_across_chunks() {
        // U+E0B0 split byte-by-byte across three advance() calls.
        let mut parser = Parser::new();
        let a1 = parser.advance(&[0xEE]);
        let a2 = parser.advance(&[0x82]);
        let a3 = parser.advance(&[0xB0]);
        assert!(a1.is_empty());
        assert!(a2.is_empty());
        assert_eq!(a3, vec![Action::Print('\u{e0b0}')]);
    }

    #[test]
    fn utf8_interrupted_by_escape_resets() {
        // Start a UTF-8 sequence then immediately get an ESC; the partial
        // bytes should be silently dropped and the ESC sequence processed.
        let mut parser = Parser::new();
        let actions = parser.advance(&[0xEE, 0x1b, b'[', b'2', b'J']);
        assert_eq!(actions, vec![Action::EraseInDisplay(2)]);
    }

    #[test]
    fn parses_bell() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x07");
        assert_eq!(actions, vec![Action::Bell]);
    }

    #[test]
    fn parses_dsr_cursor_position() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b[6n");
        assert_eq!(actions, vec![Action::DeviceStatusReport]);
    }

    #[test]
    fn dsr_only_fires_on_param_6() {
        // \x1b[5n is the "status report" query (device OK), not cursor pos — must not fire DSR.
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b[5n");
        assert!(actions.is_empty(), "unexpected actions: {actions:?}");
    }

    #[test]
    fn parses_set_cursor_shape() {
        for shape in 0u16..=6 {
            let mut parser = Parser::new();
            let input = format!("\x1b[{shape} q");
            let actions = parser.advance(input.as_bytes());
            assert_eq!(
                actions,
                vec![Action::SetCursorShape(shape)],
                "shape={shape}"
            );
        }
    }

    #[test]
    fn charset_designation_consumes_designator_byte() {
        // ESC ( B = "G0 charset = ASCII" — very common; the 'B' must NOT be
        // printed as a literal character.
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b(B");
        assert!(actions.is_empty(), "expected no actions, got {actions:?}");

        // ESC ) 0 = G1 charset = DEC Special Graphics
        let actions = parser.advance(b"\x1b)0");
        assert!(actions.is_empty(), "expected no actions, got {actions:?}");

        // Characters after the sequence must still be printed normally.
        let actions = parser.advance(b"\x1b(Babc");
        assert_eq!(
            actions,
            vec![Action::Print('a'), Action::Print('b'), Action::Print('c')]
        );
    }

    #[test]
    fn esc_m_emits_reverse_index() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1bM");
        assert_eq!(actions, vec![Action::ReverseIndex]);
    }

    #[test]
    fn parses_dec_private_mode_mouse_and_paste() {
        let mut parser = Parser::new();
        let actions = parser.advance(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?2004h");
        assert_eq!(
            actions,
            vec![
                Action::DecPrivateModeSet(1000),
                Action::DecPrivateModeSet(1002),
                Action::DecPrivateModeSet(1006),
                Action::DecPrivateModeSet(2004),
            ]
        );
    }
}
