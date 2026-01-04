use criterion::{criterion_group, criterion_main, Criterion};
use rand::{rngs::StdRng, Rng, SeedableRng};

use hypermarket_clob::matching::orderbook::{IncomingOrder, OrderBook};
use hypermarket_clob::models::{OrderType, Side, TimeInForce};

fn bench_matching(c: &mut Criterion) {
    c.bench_function("match_1m_orders", |b| {
        b.iter(|| {
            let mut book = OrderBook::new();
            let mut rng = StdRng::seed_from_u64(42);
            for i in 0..1_000_000u64 {
                let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                let price = 100 + rng.gen_range(0..10);
                let order = IncomingOrder {
                    order_id: i + 1,
                    subaccount_id: 1,
                    side,
                    order_type: OrderType::Limit,
                    tif: TimeInForce::Gtc,
                    price_ticks: price,
                    qty: 1,
                    reduce_only: false,
                    ingress_seq: i,
                };
                let _ = book.place_order(order, 10);
            }
        })
    });
}

criterion_group!(benches, bench_matching);
criterion_main!(benches);
