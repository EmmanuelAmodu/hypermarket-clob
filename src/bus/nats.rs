use std::collections::BTreeSet;

use async_nats::jetstream;
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::bus::{Bus, BusAck, BusMessage, BusSubscription};

pub struct JetStreamBus {
    jetstream: jetstream::Context,
    stream_name: String,
    durable_name: String,
}

impl JetStreamBus {
    pub async fn connect(
        url: &str,
        stream_name: String,
        subjects: Vec<String>,
        durable_name: String,
    ) -> anyhow::Result<Self> {
        let client = async_nats::connect(url).await?;
        let jetstream = jetstream::new(client);

        ensure_stream(&jetstream, &stream_name, subjects).await?;

        Ok(Self {
            jetstream,
            stream_name,
            durable_name,
        })
    }
}

#[async_trait::async_trait]
impl Bus for JetStreamBus {
    async fn publish(&self, subject: &str, payload: Bytes) -> anyhow::Result<()> {
        self.jetstream
            .publish(subject.to_string(), payload)
            .await?
            .await?;
        Ok(())
    }

    async fn subscribe(&self, subject: &str) -> anyhow::Result<BusSubscription> {
        let stream = self.jetstream.get_stream(&self.stream_name).await?;
        let consumer = stream
            .get_or_create_consumer(
                &self.durable_name,
                jetstream::consumer::pull::Config {
                    durable_name: Some(self.durable_name.clone()),
                    filter_subject: subject.to_string(),
                    ..Default::default()
                },
            )
            .await?;

        let (sender, receiver) = mpsc::channel(1024);
        tokio::spawn(async move {
            let mut messages = match consumer.messages().await {
                Ok(messages) => messages,
                Err(_) => return,
            };

            while let Some(message) = messages.next().await {
                let Ok(message) = message else { break };
                let payload = message.message.payload.clone();
                let _ = sender
                    .send(BusMessage {
                        payload,
                        ack: BusAck::Nats(message),
                    })
                    .await;
            }
        });

        Ok(BusSubscription {
            stream: ReceiverStream::new(receiver),
        })
    }

    async fn ack(&self, message: BusMessage) -> anyhow::Result<()> {
        match message.ack {
            BusAck::Nats(msg) => {
                msg.ack()
                    .await
                    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            }
            BusAck::None => {}
        }
        Ok(())
    }
}

async fn ensure_stream(
    jetstream: &jetstream::Context,
    stream_name: &str,
    subjects: Vec<String>,
) -> anyhow::Result<()> {
    if subjects.is_empty() {
        return Ok(());
    }

    let desired: BTreeSet<String> = subjects.into_iter().filter(|s| !s.is_empty()).collect();
    if desired.is_empty() {
        return Ok(());
    }

    match jetstream.get_stream(stream_name).await {
        Ok(stream) => {
            let info = stream.get_info().await?;
            let mut config = info.config;

            let mut merged: BTreeSet<String> = config.subjects.iter().cloned().collect();
            let before = merged.len();
            merged.extend(desired);
            if merged.len() != before {
                config.subjects = merged.into_iter().collect();
                jetstream.update_stream(&config).await?;
            }

            Ok(())
        }
        Err(err) => {
            let is_not_found = matches!(
                err.kind(),
                jetstream::context::GetStreamErrorKind::JetStream(js_err)
                    if js_err.error_code() == async_nats::jetstream::ErrorCode::STREAM_NOT_FOUND
            );
            if !is_not_found {
                return Err(err.into());
            }

            jetstream
                .create_stream(jetstream::stream::Config {
                    name: stream_name.to_string(),
                    subjects: desired.into_iter().collect(),
                    storage: jetstream::stream::StorageType::File,
                    ..Default::default()
                })
                .await?;
            Ok(())
        }
    }
}
