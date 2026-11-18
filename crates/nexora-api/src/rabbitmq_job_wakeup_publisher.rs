use lapin::{
    BasicProperties, Channel, Confirmation, Connection, ConnectionProperties, ExchangeKind,
    options::{
        BasicPublishOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable},
};
use nexora_platform::{JobWakeup, PlatformError};

pub const JOBS_EXCHANGE: &str = "nexora.jobs";
pub const DEAD_LETTER_EXCHANGE: &str = "nexora.jobs.dead";
pub const DEAD_LETTER_QUEUE: &str = "nexora.jobs.dead";
pub const ANALYSIS_QUEUE: &str = "nexora.jobs.analysis";
pub const RELEASE_QUEUE: &str = "nexora.jobs.release";
pub const KNOWLEDGE_QUEUE: &str = "nexora.jobs.knowledge";
pub const NOTIFICATION_QUEUE: &str = "nexora.jobs.notification";

pub struct RabbitMqJobWakeupPublisher {
    _connection: Connection,
    channel: Channel,
}

impl RabbitMqJobWakeupPublisher {
    pub fn is_connected(&self) -> bool {
        self._connection.status().connected()
    }

    pub async fn connect(uri: &str) -> Result<Self, PlatformError> {
        let connection = Connection::connect(uri, ConnectionProperties::default())
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        let channel = connection
            .create_channel()
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        Self::declare_topology(&channel).await?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(|_| PlatformError::Unavailable)?;

        Ok(Self {
            _connection: connection,
            channel,
        })
    }

    pub async fn publish(&self, wakeup: &JobWakeup) -> Result<(), PlatformError> {
        let body = serde_json::to_vec(wakeup).map_err(|_| PlatformError::Storage)?;
        let properties = BasicProperties::default()
            .with_content_type("application/json".into())
            .with_delivery_mode(2)
            .with_message_id(wakeup.job_id().into());
        let confirmation = self
            .channel
            .basic_publish(
                JOBS_EXCHANGE.into(),
                wakeup.kind().routing_key().into(),
                BasicPublishOptions {
                    mandatory: true,
                    ..BasicPublishOptions::default()
                },
                &body,
                properties,
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?
            .await
            .map_err(|_| PlatformError::Unavailable)?;

        match confirmation {
            Confirmation::Ack(None) => Ok(()),
            Confirmation::Ack(Some(_)) | Confirmation::Nack(_) | Confirmation::NotRequested => {
                Err(PlatformError::Unavailable)
            }
        }
    }

    pub(crate) async fn declare_topology(channel: &Channel) -> Result<(), PlatformError> {
        channel
            .exchange_declare(
                JOBS_EXCHANGE.into(),
                ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: true,
                    ..ExchangeDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        channel
            .exchange_declare(
                DEAD_LETTER_EXCHANGE.into(),
                ExchangeKind::Fanout,
                ExchangeDeclareOptions {
                    durable: true,
                    ..ExchangeDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        channel
            .queue_declare(
                DEAD_LETTER_QUEUE.into(),
                QueueDeclareOptions {
                    durable: true,
                    ..QueueDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        channel
            .queue_bind(
                DEAD_LETTER_QUEUE.into(),
                DEAD_LETTER_EXCHANGE.into(),
                "".into(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;

        for (queue, routing_key) in [
            (ANALYSIS_QUEUE, "nexora.jobs.analysis"),
            (RELEASE_QUEUE, "nexora.jobs.release"),
            (KNOWLEDGE_QUEUE, "nexora.jobs.knowledge"),
            (NOTIFICATION_QUEUE, "nexora.jobs.notification"),
        ] {
            Self::declare_worker_queue(channel, queue, routing_key).await?;
        }

        Ok(())
    }

    async fn declare_worker_queue(
        channel: &Channel,
        queue: &str,
        routing_key: &str,
    ) -> Result<(), PlatformError> {
        let mut arguments = FieldTable::default();
        arguments.insert(
            "x-dead-letter-exchange".into(),
            AMQPValue::LongString(DEAD_LETTER_EXCHANGE.into()),
        );
        channel
            .queue_declare(
                queue.into(),
                QueueDeclareOptions {
                    durable: true,
                    ..QueueDeclareOptions::default()
                },
                arguments,
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;
        channel
            .queue_bind(
                queue.into(),
                JOBS_EXCHANGE.into(),
                routing_key.into(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|_| PlatformError::Unavailable)?;

        Ok(())
    }
}
