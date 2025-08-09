use criterion::{Criterion, black_box, criterion_group, criterion_main};
use order_book::book::OrderBook;
use order_book::defs::items::Side;

fn setup_book() -> OrderBook {
    let mut book = OrderBook::new();
    for i in 0..1000 {
        book.add_limit_order(Side::Buy, 9999 - i, 10);
        book.add_limit_order(Side::Sell, 10001 + i, 10);
    }
    book
}

fn orderbook_benches(c: &mut Criterion) {
    c.bench_function("add_limit_order_full_match_one", |bencher| {
        let mut book = setup_book();
        bencher.iter(|| {
            book.add_limit_order(Side::Buy, black_box(10001), black_box(10));
        });
    });
}

criterion_group!(benches, orderbook_benches);
criterion_main!(benches);
