use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::bus::{Bus, BusMessage, BusSubscription};

pub struct JetStreamBus {
    client: async_nats::Client,
    _durable_name: String,
}

impl JetStreamBus {
    pub async fn connect(url: &str, durable_name: String) -> anyhow::Result<Self> {
        let client = async_nats::connect(url).await?;
        Ok(Self {
            client,
            _durable_name: durable_name,
        })
    }
}

#[async_trait::async_trait]
impl Bus for JetStreamBus {
    async fn publish(&self, subject: &str, payload: Bytes) -> anyhow::Result<()> {
        self.client.publish(subject.to_string(), payload).await?;
        Ok(())
    }

    async fn subscribe(&self, subject: &str) -> anyhow::Result<BusSubscription> {
        let (sender, receiver) = mpsc::channel(1024);
        let mut subscriber = self.client.subscribe(subject.to_string()).await?;
        tokio::spawn(async move {
            while let Some(message) = subscriber.next().await {
                let payload = Bytes::from(message.payload.to_vec());
                let _ = sender.send(BusMessage { payload }).await;
            }
        });
        Ok(BusSubscription {
            stream: ReceiverStream::new(receiver),
        })
    }

    async fn ack(&self, message: BusMessage) -> anyhow::Result<()> {
        let _ = message;
        Ok(())
    }
}
