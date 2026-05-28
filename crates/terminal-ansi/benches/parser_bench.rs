use criterion::{Criterion, black_box, criterion_group, criterion_main};
use terminal_ansi::Parser;

fn bench_parser(c: &mut Criterion) {
    let mut parser = Parser::new();
    let payload = b"\x1b[31mhello\x1b[0m\n\x1b[2J\x1b[10;20Hworld\x1b[?1049h\x1b[?1049l";

    c.bench_function("terminal_ansi/parser_advance", |b| {
        b.iter(|| {
            let actions = parser.advance(black_box(payload));
            black_box(actions.len())
        })
    });
}

criterion_group!(benches, bench_parser);
criterion_main!(benches);
