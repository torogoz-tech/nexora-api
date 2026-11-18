use crate::{RabbitMqJobWakeupDelivery, RabbitMqJobWakeupPublisher};
use futures_lite::StreamExt;
use lapin::{
    Connection, ConnectionProperties,
    options::{BasicConsumeOptions, BasicQosOptions},
    types::FieldTable,
};
use nexora_platform::{JobKind, JobWakeup, PlatformError};

pub struct RabbitMqJobWakeupConsumer {
    _connection: Connection,
    consumer: lapin::Consumer,
    expected_kind: JobKind,
}

impl RabbitMqJobWakeupConsumer {
    pub async fn connect(uri: &str, expected_kind: JobKind) -> Result<Self, PlatformError> {
        let connection = Connection::connect(uri, ConnectionProperties::default())
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        let channel = connection
            .create_channel()
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        RabbitMqJobWakeupPublisher::declare_topology(&channel).await?;
        channel
            .basic_qos(1, BasicQosOptions::default())
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        let consumer = channel
            .basic_consume(
                expected_kind.queue_name().into(),
                "".into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;

        Ok(Self {
            _connection: connection,
            consumer,
            expected_kind,
        })
    }

    pub async fn next(&mut self) -> Result<Option<RabbitMqJobWakeupDelivery>, PlatformError> {
        loop {
            let Some(delivery) = self.consumer.next().await else {
                return Ok(None);
            };
            let delivery = delivery.map_err(|_| PlatformError::Unavailable)?;
            let wakeup =
                match serde_json::from_slice::<JobWakeup>(&delivery.data) {
                    Ok(wakeup)
                        if wakeup.kind().queue_name() == self.expected_kind.queue_name()
                            && delivery.properties.message_id().as_ref().is_some_and(
                                |message_id| message_id.as_str() == wakeup.job_id(),
                            ) =>
                    {
                        wakeup
                    }
                    _ => {
                        delivery
                            .reject(lapin::options::BasicRejectOptions { requeue: false })
                            .await
                            .map_err(|_| PlatformError::Unavailable)?;
                        continue;
                    }
                };

            return Ok(Some(RabbitMqJobWakeupDelivery::new(delivery, wakeup)));
        }
    }
}
