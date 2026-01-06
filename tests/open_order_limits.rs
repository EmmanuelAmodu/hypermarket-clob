use std::path::PathBuf;

use hypermarket_clob::config::{MarketConfig, MatchingMode};
use hypermarket_clob::engine::EngineShard;
use hypermarket_clob::models::{CancelOrder, Event, EventEnvelope, NewOrder, OrderAck, OrderStatus, OrderType, Side, TimeInForce};
use hypermarket_clob::persistence::wal::Wal;
use hypermarket_clob::risk::{RiskConfig, RiskEngine};

fn market_config(max_subaccount: u64) -> MarketConfig {
    MarketConfig {
        market_id: 1,
        tick_size: 1,
        lot_size: 1,
        maker_fee_bps: 0,
        taker_fee_bps: 0,
        initial_margin_bps: 0,
        maintenance_margin_bps: 0,
        max_position: 1_000_000,
        price_band_bps: 10_000,
        max_open_orders_per_subaccount: max_subaccount,
        matching_mode: MatchingMode::Continuous,
        batch_interval_ms: 2000,
    }
}

fn new_shard(max_subaccount: u64) -> EngineShard {
    let wal_path = PathBuf::from(std::env::temp_dir().join(format!(
        "open_order_limits_{:x}.wal",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )));
    let wal = Wal::open(&wal_path).unwrap();
    let risk = RiskEngine::new(RiskConfig {
        max_slippage_bps: 50,
        max_leverage: 10,
    });
    EngineShard::new(0, vec![market_config(max_subaccount)], wal, risk)
}

fn ack_from_outputs(outputs: &[EventEnvelope]) -> OrderAck {
    for env in outputs {
        if let Event::OrderAck(ack) = &env.event {
            return ack.clone();
        }
    }
    panic!("missing OrderAck");
}

fn gtc_order(request_id: &str, subaccount_id: u64, side: Side) -> NewOrder {
    NewOrder {
        request_id: request_id.to_string(),
        market_id: 1,
        subaccount_id,
        side,
        order_type: OrderType::Limit,
        tif: TimeInForce::Gtc,
        price_ticks: 1,
        qty: 1,
        reduce_only: false,
        expiry_ts: 0,
        nonce: 0,
        client_ts: 0,
    }
}

fn ioc_order(request_id: &str, subaccount_id: u64, side: Side) -> NewOrder {
    NewOrder {
        request_id: request_id.to_string(),
        market_id: 1,
        subaccount_id,
        side,
        order_type: OrderType::Limit,
        tif: TimeInForce::Ioc,
        price_ticks: 1,
        qty: 1,
        reduce_only: false,
        expiry_ts: 0,
        nonce: 0,
        client_ts: 0,
    }
}

#[test]
fn enforces_max_open_orders_per_subaccount() {
    let mut shard = new_shard(1);

    let a1 = ack_from_outputs(&shard.handle_event(Event::NewOrder(gtc_order("r1", 1, Side::Buy)), 1).unwrap());
    assert_eq!(a1.status, OrderStatus::Accepted);

    let a2 = ack_from_outputs(&shard.handle_event(Event::NewOrder(gtc_order("r2", 1, Side::Buy)), 2).unwrap());
    assert_eq!(a2.status, OrderStatus::Rejected);
    assert_eq!(a2.reject_reason.as_deref(), Some("max open orders per subaccount"));

    let a3 = ack_from_outputs(&shard.handle_event(Event::NewOrder(gtc_order("r3", 2, Side::Buy)), 3).unwrap());
    assert_eq!(a3.status, OrderStatus::Accepted);
}

#[test]
fn filled_maker_frees_subaccount_open_order_slot() {
    let mut shard = new_shard(1);

    let maker = ack_from_outputs(&shard.handle_event(Event::NewOrder(gtc_order("maker", 2, Side::Sell)), 1).unwrap());
    assert_eq!(maker.status, OrderStatus::Accepted);

    let taker = ack_from_outputs(&shard.handle_event(Event::NewOrder(ioc_order("taker", 1, Side::Buy)), 2).unwrap());
    assert_eq!(taker.status, OrderStatus::Accepted);

    let maker2 = ack_from_outputs(&shard.handle_event(Event::NewOrder(gtc_order("maker2", 2, Side::Sell)), 3).unwrap());
    assert_eq!(maker2.status, OrderStatus::Accepted);
}

#[test]
fn ioc_no_fill_does_not_leave_owner_entry() {
    let mut shard = new_shard(0);
    let ack = ack_from_outputs(&shard.handle_event(Event::NewOrder(ioc_order("ioc", 1, Side::Buy)), 1).unwrap());
    let order_id = ack.assigned_order_id.expect("assigned order id");
    assert!(!shard.order_owners.contains_key(&order_id));

    let cancel = CancelOrder {
        request_id: "cancel".to_string(),
        market_id: 1,
        subaccount_id: 1,
        order_id: Some(order_id),
        nonce_start: None,
        nonce_end: None,
    };
    let outputs = shard.handle_event(Event::CancelOrder(cancel), 2).unwrap();
    assert!(outputs.is_empty());
}
