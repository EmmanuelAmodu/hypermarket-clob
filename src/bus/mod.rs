use bytes::Bytes;

#[async_trait::async_trait]
pub trait Bus: Send + Sync {
    async fn publish(&self, subject: &str, payload: Bytes) -> anyhow::Result<()>;
    async fn subscribe(&self, subject: &str) -> anyhow::Result<BusSubscription>;
    async fn ack(&self, _message: BusMessage) -> anyhow::Result<()>;
}

pub struct BusMessage {
    pub payload: Bytes,
}

pub struct BusSubscription {
    pub stream: tokio_stream::wrappers::ReceiverStream<BusMessage>,
}

pub mod nats;
