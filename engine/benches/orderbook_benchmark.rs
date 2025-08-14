use criterion::{Criterion, black_box, criterion_group, criterion_main};
use engine::book::OrderBook;
use engine::messages::trading::Side;

fn setup_book() -> OrderBook {
    let mut book = OrderBook::new();
    for i in 0..1000 {
        book.add_limit_order(Side::Buy, 9999 - i, 10);
        book.add_limit_order(Side::Sell, 10001 + i, 10);
    }
    book
}

fn orderbook_benches(c: &mut Criterion) {
    c.bench_function("add_limit_order_no_match", |bencher| {
        bencher.iter_batched(
            || setup_book(),
            |mut book| {
                book.add_limit_order(Side::Buy, black_box(9000), black_box(10));
            },
            criterion::BatchSize::PerIteration,
        );
    });

    c.bench_function("add_limit_order_full_match_one", |bencher| {
        bencher.iter_batched(
            || setup_book(),
            |mut book| {
                book.add_limit_order(Side::Buy, black_box(10001), black_box(10));
            },
            criterion::BatchSize::PerIteration,
        );
    });

    c.bench_function("add_limit_order_walk_the_book", |bencher| {
        bencher.iter_batched(
            || setup_book(),
            |mut book| {
                book.add_limit_order(Side::Buy, black_box(10005), black_box(50));
            },
            criterion::BatchSize::PerIteration,
        );
    });
}

criterion_group!(benches, orderbook_benches);
criterion_main!(benches);
