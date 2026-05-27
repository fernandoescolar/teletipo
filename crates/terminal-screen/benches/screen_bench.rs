use criterion::{black_box, criterion_group, criterion_main, Criterion};
use terminal_screen::Screen;

fn bench_screen_apply(c: &mut Criterion) {
    c.bench_function("terminal_screen/write_and_scroll", |b| {
        b.iter(|| {
            let mut screen = Screen::new(40, 120);
            for _ in 0..1000 {
                for ch in "benchmark-line-1234567890".chars() {
                    screen.put_char(ch);
                }
                screen.linefeed();
                screen.carriage_return();
            }
            black_box(screen.scrollback_len())
        })
    });
}

criterion_group!(benches, bench_screen_apply);
criterion_main!(benches);
