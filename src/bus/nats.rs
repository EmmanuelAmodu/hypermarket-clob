use async_nats::jetstream;
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::bus::{Bus, BusAck, BusMessage, BusSubscription};

pub struct JetStreamBus {
    client: async_nats::Client,
    context: jetstream::Context,
    durable_name: String,
}

impl JetStreamBus {
    pub async fn connect(url: &str, durable_name: String) -> anyhow::Result<Self> {
        let client = async_nats::connect(url).await?;
        let context = jetstream::new(client.clone());
        Ok(Self {
            client,
            context,
            durable_name,
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
        let durable = self.durable_name.clone();
        let mut consumer = self
            .context
            .create_or_get_consumer(
                subject,
                jetstream::consumer::pull::Config {
                    durable_name: Some(durable.clone()),
                    ack_policy: jetstream::consumer::AckPolicy::Explicit,
                    ..Default::default()
                },
            )
            .await?;
        tokio::spawn(async move {
            loop {
                let mut messages = match consumer.fetch(10).await {
                    Ok(messages) => messages,
                    Err(_) => continue,
                };
                while let Some(message) = messages.next().await {
                    if let Ok(message) = message {
                        let payload = Bytes::from(message.payload.to_vec());
                        let _ = sender
                            .send(BusMessage {
                                payload,
                                ack: BusAck::Nats(message),
                            })
                            .await;
                    }
                }
            }
        });
        Ok(BusSubscription {
            stream: ReceiverStream::new(receiver),
        })
    }

    async fn ack(&self, message: BusMessage) -> anyhow::Result<()> {
        match message.ack {
            BusAck::Nats(msg) => {
                msg.ack().await?;
            }
            BusAck::None => {}
        }
        Ok(())
    }
}
