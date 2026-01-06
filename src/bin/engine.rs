use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use hypermarket_clob::bus::nats::JetStreamBus;
use hypermarket_clob::config::Settings;
use hypermarket_clob::engine::router::run_router;
use hypermarket_clob::metrics::install_recorder;

#[derive(Parser, Debug)]
#[command(name = "engine")]
struct Args {
    #[arg(long, default_value = "config/example.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();
    let _prom = install_recorder()?;

    let args = Args::parse();
    let settings = Settings::load(&args.config)?;
    let bus = JetStreamBus::connect(
        &settings.bus.nats_url,
        settings.bus.stream_name.clone(),
        vec![settings.bus.input_subject.clone(), settings.bus.output_subject.clone()],
        settings.bus.durable_name.clone(),
    )
    .await?;
    run_router(settings, Arc::new(bus)).await
}
