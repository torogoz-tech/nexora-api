use lapin::{message::Delivery, options::BasicAckOptions, options::BasicNackOptions};
use nexora_platform::{JobWakeup, PlatformError};

pub struct RabbitMqJobWakeupDelivery {
    delivery: Delivery,
    wakeup: JobWakeup,
}

impl RabbitMqJobWakeupDelivery {
    pub(crate) fn new(delivery: Delivery, wakeup: JobWakeup) -> Self {
        Self { delivery, wakeup }
    }

    pub fn wakeup(&self) -> &JobWakeup {
        &self.wakeup
    }

    pub fn is_persistent(&self) -> bool {
        *self.delivery.properties.delivery_mode() == Some(2)
    }

    pub async fn ack(self) -> Result<(), PlatformError> {
        self.delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|_| PlatformError::Unavailable)
            .map(|_| ())
    }

    pub async fn retry(self) -> Result<(), PlatformError> {
        self.delivery
            .nack(BasicNackOptions {
                requeue: true,
                ..BasicNackOptions::default()
            })
            .await
            .map_err(|_| PlatformError::Unavailable)
            .map(|_| ())
    }
}
