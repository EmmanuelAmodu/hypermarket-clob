use std::path::PathBuf;

use clap::Parser;

use hypermarket_clob::config::Settings;
use hypermarket_clob::engine::shard::EngineShard;
use hypermarket_clob::persistence::snapshot::SnapshotStore;
use hypermarket_clob::persistence::wal::Wal;
use hypermarket_clob::risk::{RiskConfig, RiskEngine};

#[derive(Parser, Debug)]
#[command(name = "replay")]
struct Args {
    #[arg(long)]
    config: String,
    #[arg(long)]
    log: String,
    #[arg(long)]
    snapshot: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let settings = Settings::load(&args.config)?;
    let log_path = PathBuf::from(&args.log);

    let snapshot = args
        .snapshot
        .as_ref()
        .map(PathBuf::from)
        .map(|path| SnapshotStore::load(&path))
        .transpose()?
        .flatten();

    let replay_path = std::env::temp_dir().join("replay.wal");
    let wal = Wal::open(&replay_path)?;
    let risk = RiskEngine::new(RiskConfig {
        max_slippage_bps: 50,
        max_leverage: 10,
    });

    let mut shard = if let Some(snapshot) = snapshot {
        EngineShard::restore(snapshot.state, settings.markets.clone(), wal, risk)
    } else {
        EngineShard::new(0, settings.markets.clone(), wal, risk)
    };

    let events = Wal::load(&log_path)?;
    for envelope in events {
        if matches!(envelope.event, hypermarket_clob::models::Event::NewOrder(_) | hypermarket_clob::models::Event::CancelOrder(_) | hypermarket_clob::models::Event::PriceUpdate(_) | hypermarket_clob::models::Event::FundingUpdate(_)) {
            let _ = shard.handle_event(envelope.event, envelope.ts);
        }
    }

    let state = shard.snapshot();
    let state_bytes = bincode::serialize(&state)?;
    let hash = blake3::hash(&state_bytes);
    println!("state_hash={}", hash.to_hex());
    Ok(())
}
