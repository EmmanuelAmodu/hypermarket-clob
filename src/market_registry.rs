use futures::TryStreamExt;

use crate::config::MarketConfig;

pub async fn load_all(nats_url: &str, bucket: &str) -> anyhow::Result<Vec<MarketConfig>> {
    let client = async_nats::connect(nats_url).await?;
    let jetstream = async_nats::jetstream::new(client);
    let kv = jetstream
        .create_key_value(async_nats::jetstream::kv::Config {
            bucket: bucket.to_string(),
            history: 1,
            storage: async_nats::jetstream::stream::StorageType::File,
            ..Default::default()
        })
        .await?;

    let keys = kv.keys().await?.try_collect::<Vec<String>>().await?;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if let Some(value) = kv.get(key).await? {
            let market: MarketConfig = serde_json::from_slice(&value)?;
            out.push(market);
        }
    }
    Ok(out)
}

pub async fn watch_updates<F>(nats_url: &str, bucket: &str, mut on_market: F) -> anyhow::Result<()>
where
    F: FnMut(MarketConfig) + Send + 'static,
{
    use futures::StreamExt;

    let client = async_nats::connect(nats_url).await?;
    let jetstream = async_nats::jetstream::new(client);
    let kv = jetstream
        .create_key_value(async_nats::jetstream::kv::Config {
            bucket: bucket.to_string(),
            history: 1,
            storage: async_nats::jetstream::stream::StorageType::File,
            ..Default::default()
        })
        .await?;

    let mut watch = kv.watch_all().await?;
    while let Some(entry) = watch.next().await {
        let entry = entry?;
        if entry.operation != async_nats::jetstream::kv::Operation::Put {
            continue;
        }
        let market: MarketConfig = serde_json::from_slice(&entry.value)?;
        on_market(market);
    }
    Ok(())
}

pub async fn watch_updates_tx(
    nats_url: String,
    bucket: String,
    tx: tokio::sync::mpsc::Sender<MarketConfig>,
) -> anyhow::Result<()> {
    use futures::StreamExt;

    let client = async_nats::connect(nats_url).await?;
    let jetstream = async_nats::jetstream::new(client);
    let kv = jetstream
        .create_key_value(async_nats::jetstream::kv::Config {
            bucket,
            history: 1,
            storage: async_nats::jetstream::stream::StorageType::File,
            ..Default::default()
        })
        .await?;

    let mut watch = kv.watch_all().await?;
    while let Some(entry) = watch.next().await {
        let entry = entry?;
        if entry.operation != async_nats::jetstream::kv::Operation::Put {
            continue;
        }
        let market: MarketConfig = serde_json::from_slice(&entry.value)?;
        if tx.send(market).await.is_err() {
            break;
        }
    }
    Ok(())
}
