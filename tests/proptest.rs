use std::path::PathBuf;

use proptest::prelude::*;

use hypermarket_clob::config::{MarketConfig, MatchingMode};
use hypermarket_clob::engine::shard::EngineShard;
use hypermarket_clob::models::{Event, NewOrder, OrderType, Side, TimeInForce};
use hypermarket_clob::persistence::wal::Wal;
use hypermarket_clob::risk::{RiskConfig, RiskEngine};

fn market() -> MarketConfig {
    MarketConfig {
        market_id: 1,
        tick_size: 1,
        lot_size: 1,
        maker_fee_bps: 1,
        taker_fee_bps: 2,
        initial_margin_bps: 1,
        maintenance_margin_bps: 1,
        max_position: 1000,
        price_band_bps: 10_000,
        max_open_orders_per_subaccount: 0,
        matching_mode: MatchingMode::Continuous,
        batch_interval_ms: 2000,
    }
}

proptest! {
    #[test]
    fn determinism_replay(seq in 1u64..100u64) {
        let wal_path = PathBuf::from(std::env::temp_dir().join("prop.wal"));
        let wal = Wal::open(&wal_path).unwrap();
        let risk = RiskEngine::new(RiskConfig { max_slippage_bps: 50, max_leverage: 10 });
        let mut shard = EngineShard::new(0, vec![market()], wal, risk);
        for i in 0..seq {
            let order = NewOrder {
                request_id: format!("req-{i}"),
                market_id: 1,
                subaccount_id: 1,
                side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
                order_type: OrderType::Limit,
                tif: TimeInForce::Gtc,
                price_ticks: 100,
                qty: 1,
                reduce_only: false,
                expiry_ts: 0,
                nonce: i,
                client_ts: 0,
            };
            let _ = shard.handle_event(Event::NewOrder(order), 0);
        }
        let state_hash = blake3::hash(&bincode::serialize(&shard.snapshot()).unwrap());
        let state_hash_again = blake3::hash(&bincode::serialize(&shard.snapshot()).unwrap());
        prop_assert_eq!(state_hash, state_hash_again);
    }
}
