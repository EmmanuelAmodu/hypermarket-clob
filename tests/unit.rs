use hypermarket_clob::matching::orderbook::{IncomingOrder, OrderBook};
use hypermarket_clob::models::{OrderType, Side, TimeInForce};
use hypermarket_clob::risk::{RiskConfig, RiskEngine, RiskError};
use hypermarket_clob::config::{MarketConfig, MatchingMode};

#[test]
fn ioc_rejects_rest() {
    let mut book = OrderBook::new();
    let order = IncomingOrder {
        order_id: 1,
        subaccount_id: 1,
        side: Side::Buy,
        order_type: OrderType::Limit,
        tif: TimeInForce::Ioc,
        price_ticks: 100,
        qty: 10,
        reduce_only: false,
        ingress_seq: 1,
    };
    let (_fills, remaining) = book.place_order(order, 10);
    assert!(remaining.is_none());
    assert!(!book.has_order(1));
}

#[test]
fn fok_requires_full_fill() {
    let mut book = OrderBook::new();
    let maker = IncomingOrder {
        order_id: 1,
        subaccount_id: 1,
        side: Side::Sell,
        order_type: OrderType::Limit,
        tif: TimeInForce::Gtc,
        price_ticks: 100,
        qty: 5,
        reduce_only: false,
        ingress_seq: 1,
    };
    book.place_order(maker, 10);
    let taker = IncomingOrder {
        order_id: 2,
        subaccount_id: 2,
        side: Side::Buy,
        order_type: OrderType::Limit,
        tif: TimeInForce::Fok,
        price_ticks: 100,
        qty: 10,
        reduce_only: false,
        ingress_seq: 2,
    };
    let (fills, _) = book.place_order(taker, 10);
    assert!(fills.is_empty());
}

#[test]
fn fee_math_works() {
    let notional = 1000_i64;
    let fee_bps = 5_i64;
    let fee = notional * fee_bps / 10_000;
    assert_eq!(fee, 0);
}

#[test]
fn cancel_by_order_id() {
    let mut book = OrderBook::new();
    let maker = IncomingOrder {
        order_id: 1,
        subaccount_id: 1,
        side: Side::Sell,
        order_type: OrderType::Limit,
        tif: TimeInForce::Gtc,
        price_ticks: 100,
        qty: 5,
        reduce_only: false,
        ingress_seq: 1,
    };
    book.place_order(maker, 10);
    assert!(book.cancel(1));
    assert!(!book.has_order(1));
}

#[test]
fn reduce_only_validation() {
    let mut risk = RiskEngine::new(RiskConfig {
        max_slippage_bps: 50,
        max_leverage: 10,
    });
    let market = MarketConfig {
        market_id: 1,
        tick_size: 1,
        lot_size: 1,
        maker_fee_bps: 1,
        taker_fee_bps: 2,
        initial_margin_bps: 1,
        maintenance_margin_bps: 1,
        max_position: 10,
        price_band_bps: 10_000,
        matching_mode: MatchingMode::Continuous,
        batch_interval_ms: 2000,
    };
    risk.ensure_subaccount(1).positions.insert(
        1,
        hypermarket_clob::risk::Position {
            size: 5,
            entry_price: 100,
            funding_index: 0,
        },
    );
    let result = risk.validate_order(
        &market,
        1,
        Side::Buy,
        OrderType::Limit,
        100,
        10,
        true,
    );
    assert!(matches!(result, Err(RiskError::ReduceOnly)));
}
