use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub bus: BusConfig,
    pub shard_count: usize,
    pub markets: Vec<MarketConfig>,
    pub persistence: PersistenceConfig,
    pub snapshot_interval_secs: u64,
    pub book_delta_levels: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BusConfig {
    pub nats_url: String,
    pub input_subject: String,
    pub output_subject: String,
    pub durable_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketConfig {
    pub market_id: u64,
    pub tick_size: u64,
    pub lot_size: u64,
    pub maker_fee_bps: i64,
    pub taker_fee_bps: i64,
    pub initial_margin_bps: u64,
    pub maintenance_margin_bps: u64,
    pub max_position: i64,
    pub price_band_bps: u64,
    pub matching_mode: MatchingMode,
    pub batch_interval_ms: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchingMode {
    Batch,
    Continuous,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersistenceConfig {
    pub wal_path: String,
    pub snapshot_path: String,
}

impl Settings {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path));
        Ok(builder.build()?.try_deserialize()?)
    }
}
