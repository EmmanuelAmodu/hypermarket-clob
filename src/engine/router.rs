use std::sync::Arc;

use bytes::Bytes;
use prost::Message;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::bus::Bus;
use crate::config::Settings;
use crate::engine::shard::EngineShard;
use crate::market_registry;
use crate::models::{pb, Event};
use crate::persistence::wal::Wal;
use crate::risk::{RiskConfig, RiskEngine};

pub async fn run_router(settings: Settings, bus: Arc<dyn Bus>) -> anyhow::Result<()> {
    let mut shard_senders = Vec::new();
    let mut shard_tasks = Vec::new();

    let mut markets = settings.markets.clone();
    if let Ok(dynamic) = market_registry::load_all(&settings.bus.nats_url, &settings.bus.markets_bucket).await {
        let mut by_id = std::collections::HashMap::<u64, crate::config::MarketConfig>::new();
        for m in markets.drain(..) {
            by_id.insert(m.market_id, m);
        }
        for m in dynamic {
            by_id.insert(m.market_id, m);
        }
        markets = by_id.into_values().collect();
    }

    enum ShardMsg {
        Event { event: Event, ts: u64, message: crate::bus::BusMessage },
        MarketUpdate(crate::config::MarketConfig),
    }

    for shard_id in 0..settings.shard_count {
        let (tx, mut rx) = mpsc::channel::<ShardMsg>(1024);
        shard_senders.push(tx);

        let shard_markets: Vec<_> = markets
            .iter()
            .filter(|m| (m.market_id as usize) % settings.shard_count == shard_id)
            .cloned()
            .collect();
        let wal = Wal::open(std::path::Path::new(&settings.persistence.wal_path))?;
        let risk = RiskEngine::new(RiskConfig {
            max_slippage_bps: 50,
            max_leverage: 10,
        });
        let mut shard = EngineShard::new(shard_id, shard_markets, wal, risk);
        let output_subject = settings.bus.output_subject.clone();
        let bus_clone = Arc::clone(&bus);
        let handle = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    ShardMsg::Event { event, ts, message } => match shard.handle_event(event, ts) {
                        Ok(outputs) => {
                            for output in outputs {
                                let bytes = encode_output(output);
                                let _ = bus_clone.publish(&output_subject, bytes).await;
                            }
                            let _ = bus_clone.ack(message).await;
                        }
                        Err(_) => {
                            // Do not ack; allow redelivery.
                        }
                    },
                    ShardMsg::MarketUpdate(market) => {
                        shard.upsert_market(market);
                    }
                }
            }
        });
        shard_tasks.push(handle);
    }

    // Watch for dynamic market updates and apply to the owning shard.
    {
        let (tx, mut rx) = mpsc::channel::<crate::config::MarketConfig>(1024);
        tokio::spawn(market_registry::watch_updates_tx(
            settings.bus.nats_url.clone(),
            settings.bus.markets_bucket.clone(),
            tx,
        ));

        let senders = shard_senders.clone();
        tokio::spawn(async move {
            while let Some(market) = rx.recv().await {
                let shard_id = (market.market_id as usize) % senders.len();
                if let Some(sender) = senders.get(shard_id) {
                    let _ = sender.send(ShardMsg::MarketUpdate(market)).await;
                }
            }
        });
    }

    let mut subscription = bus.subscribe(&settings.bus.input_subject).await?;
    while let Some(message) = subscription.stream.next().await {
        let payload = message.payload.clone();
        let ts = current_ts();
        if let Ok(event) = decode_input(payload) {
            let market_id = market_id_for_event(&event).unwrap_or(0);
            let shard_id = (market_id as usize) % settings.shard_count;
            if let Some(sender) = shard_senders.get(shard_id) {
                if sender
                    .send(ShardMsg::Event {
                        event,
                        ts,
                        message,
                    })
                    .await
                    .is_err()
                {
                    warn!("failed to forward input event to shard");
                }
            } else {
                warn!("no shard sender for input event");
                let _ = bus.ack(message).await;
            }
        } else {
            warn!("failed to decode input event");
            let _ = bus.ack(message).await;
        }
    }

    info!("router stopped");
    for task in shard_tasks {
        let _ = task.await;
    }
    Ok(())
}

fn decode_input(payload: Bytes) -> anyhow::Result<Event> {
    let input = pb::InputEvent::decode(payload)?;
    let event = match input.payload.ok_or_else(|| anyhow::anyhow!("missing payload"))? {
        pb::input_event::Payload::NewOrder(order) => Event::NewOrder(order.into()),
        pb::input_event::Payload::CancelOrder(cancel) => Event::CancelOrder(cancel.into()),
        pb::input_event::Payload::PriceUpdate(update) => Event::PriceUpdate(update.into()),
        pb::input_event::Payload::FundingUpdate(update) => Event::FundingUpdate(update.into()),
    };
    Ok(event)
}

fn encode_output(envelope: crate::models::EventEnvelope) -> Bytes {
    let output = match envelope.event {
        Event::OrderAck(ack) => pb::OutputEvent {
            payload: Some(pb::output_event::Payload::OrderAck(ack.into())),
        },
        Event::Fill(fill) => pb::OutputEvent {
            payload: Some(pb::output_event::Payload::Fill(fill.into())),
        },
        Event::BookDelta(delta) => pb::OutputEvent {
            payload: Some(pb::output_event::Payload::BookDelta(delta.into())),
        },
        Event::SettlementBatch(batch) => pb::OutputEvent {
            payload: Some(pb::output_event::Payload::SettlementBatch(batch.into())),
        },
        _ => pb::OutputEvent { payload: None },
    };
    Bytes::from(output.encode_to_vec())
}

fn market_id_for_event(event: &Event) -> Option<u64> {
    match event {
        Event::NewOrder(order) => Some(order.market_id),
        Event::CancelOrder(order) => Some(order.market_id),
        Event::PriceUpdate(update) => Some(update.market_id),
        Event::FundingUpdate(update) => Some(update.market_id),
        _ => None,
    }
}

fn current_ts() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
