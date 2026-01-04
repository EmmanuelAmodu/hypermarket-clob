fn main() {
    let mut engine = hypermarket_clob::MatchingEngine::new();
    let market = "demo";

    let ask = hypermarket_clob::NewOrder::Limit {
        user_id: 1,
        side: hypermarket_clob::Side::Ask,
        price: 100,
        quantity: 5,
        time_in_force: hypermarket_clob::TimeInForce::Gtc,
    };
    let bid = hypermarket_clob::NewOrder::Limit {
        user_id: 2,
        side: hypermarket_clob::Side::Bid,
        price: 105,
        quantity: 3,
        time_in_force: hypermarket_clob::TimeInForce::Gtc,
    };

    let r1 = engine.place_order(market, ask).unwrap();
    let r2 = engine.place_order(market, bid).unwrap();

    println!("r1: {r1:?}");
    println!("r2: {r2:?}");
    println!("top: {:?}", engine.snapshot(market));
}
