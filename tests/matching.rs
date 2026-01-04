use hypermarket_clob::{MatchingEngine, NewOrder, Side, TimeInForce};

#[test]
fn limit_match_executes_at_maker_price() {
    let mut engine = MatchingEngine::new();
    let market = "m1";

    let ask = NewOrder::Limit {
        user_id: 1,
        side: Side::Ask,
        price: 100,
        quantity: 5,
        time_in_force: TimeInForce::Gtc,
    };
    engine.place_order(market, ask).unwrap();

    let bid = NewOrder::Limit {
        user_id: 2,
        side: Side::Bid,
        price: 105,
        quantity: 3,
        time_in_force: TimeInForce::Gtc,
    };
    let report = engine.place_order(market, bid).unwrap();
    assert_eq!(report.fills().len(), 1);
    assert_eq!(report.fills()[0].price, 100);
    assert_eq!(report.fills()[0].quantity, 3);

    let top = engine.snapshot(market);
    assert_eq!(top.best_ask, Some((100, 2)));
    assert_eq!(top.best_bid, None);
}

#[test]
fn price_time_priority_is_fifo_within_level() {
    let mut engine = MatchingEngine::new();
    let market = "m1";

    let r1 = engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 10,
                side: Side::Ask,
                price: 100,
                quantity: 2,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();
    let first_maker_id = r1.order_id();

    let r2 = engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 11,
                side: Side::Ask,
                price: 100,
                quantity: 2,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();
    let second_maker_id = r2.order_id();

    let taker = engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 99,
                side: Side::Bid,
                price: 100,
                quantity: 3,
                time_in_force: TimeInForce::Ioc,
            },
        )
        .unwrap();

    assert_eq!(taker.fills().len(), 2);
    assert_eq!(taker.fills()[0].maker_order_id, first_maker_id);
    assert_eq!(taker.fills()[0].quantity, 2);
    assert_eq!(taker.fills()[1].maker_order_id, second_maker_id);
    assert_eq!(taker.fills()[1].quantity, 1);

    let top = engine.snapshot(market);
    assert_eq!(top.best_ask, Some((100, 1)));
}

#[test]
fn ioc_does_not_post_remainder() {
    let mut engine = MatchingEngine::new();
    let market = "m1";

    let report = engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 2,
                side: Side::Bid,
                price: 100,
                quantity: 5,
                time_in_force: TimeInForce::Ioc,
            },
        )
        .unwrap();

    assert!(report.fills().is_empty());
    let top = engine.snapshot(market);
    assert_eq!(top.best_bid, None);
    assert_eq!(top.best_ask, None);
}

#[test]
fn gtc_posts_remainder_after_partial_fill() {
    let mut engine = MatchingEngine::new();
    let market = "m1";

    engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 1,
                side: Side::Ask,
                price: 100,
                quantity: 2,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();

    let report = engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 2,
                side: Side::Bid,
                price: 100,
                quantity: 5,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();

    assert_eq!(report.fills().len(), 1);
    assert_eq!(report.fills()[0].quantity, 2);

    let top = engine.snapshot(market);
    assert_eq!(top.best_ask, None);
    assert_eq!(top.best_bid, Some((100, 3)));
}

#[test]
fn fok_rejects_if_not_fillable() {
    let mut engine = MatchingEngine::new();
    let market = "m1";

    engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 1,
                side: Side::Ask,
                price: 100,
                quantity: 2,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();

    let err = engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 2,
                side: Side::Bid,
                price: 100,
                quantity: 5,
                time_in_force: TimeInForce::Fok,
            },
        )
        .unwrap_err();

    assert_eq!(format!("{err:?}"), "FokNotFillable");
    let top = engine.snapshot(market);
    assert_eq!(top.best_ask, Some((100, 2)));
    assert_eq!(top.best_bid, None);
}

#[test]
fn market_order_matches_and_cancels_remainder() {
    let mut engine = MatchingEngine::new();
    let market = "m1";

    engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 1,
                side: Side::Ask,
                price: 100,
                quantity: 2,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();
    engine
        .place_order(
            market,
            NewOrder::Limit {
                user_id: 2,
                side: Side::Ask,
                price: 101,
                quantity: 2,
                time_in_force: TimeInForce::Gtc,
            },
        )
        .unwrap();

    let report = engine
        .place_order(
            market,
            NewOrder::Market {
                user_id: 99,
                side: Side::Bid,
                quantity: 5,
                time_in_force: TimeInForce::Ioc,
            },
        )
        .unwrap();

    assert_eq!(report.fills().len(), 2);
    assert_eq!(report.fills()[0].price, 100);
    assert_eq!(report.fills()[0].quantity, 2);
    assert_eq!(report.fills()[1].price, 101);
    assert_eq!(report.fills()[1].quantity, 2);

    let top = engine.snapshot(market);
    assert_eq!(top.best_ask, None);
    assert_eq!(top.best_bid, None);
}
