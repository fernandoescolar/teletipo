use terminal_core::TerminalSession;

#[test]
fn stress_large_pty_like_stream_stays_stable() {
    let mut session = TerminalSession::new(40, 120).expect("session");

    for i in 0..100_000 {
        let line = format!("line-{i:05} value=abcdef1234567890\\n\\r");
        session.feed(line.as_bytes());
    }

    let snapshot = session.snapshot_text();
    assert!(!snapshot.is_empty());
    assert!(snapshot.contains("line-"));
}

#[test]
fn stress_alternate_buffer_toggles() {
    let mut session = TerminalSession::new(24, 80).expect("session");

    for _ in 0..500 {
        session.feed(b"\\x1b[?1049h");
        session.feed(b"ALT");
        session.feed(b"\\x1b[?1049l");
        session.feed(b"MAIN");
    }

    let snapshot = session.snapshot_text();
    assert!(snapshot.contains("MAIN"));
}
