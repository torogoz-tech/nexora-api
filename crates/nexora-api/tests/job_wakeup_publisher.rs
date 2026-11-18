use lapin::{Connection, ConnectionProperties};
use nexora_api::{ANALYSIS_QUEUE, RabbitMqJobWakeupConsumer, RabbitMqJobWakeupPublisher};
use nexora_platform::{JobKind, JobWakeup};

#[tokio::test]
async fn publishes_a_confirmed_persistent_analysis_wakeup() {
    let uri = std::env::var("NEXORA_TEST_RABBITMQ_URL")
        .expect("NEXORA_TEST_RABBITMQ_URL must point to the CI RabbitMQ service");
    let publisher = RabbitMqJobWakeupPublisher::connect(&uri)
        .await
        .expect("publisher should connect and declare durable topology");
    let wakeup = JobWakeup::new("job_contract_test".into(), JobKind::RepositoryIndex)
        .expect("fixture job ID is valid");
    let connection = Connection::connect(&uri, ConnectionProperties::default())
        .await
        .expect("consumer connection should open");
    let channel = connection
        .create_channel()
        .await
        .expect("consumer channel should open");
    channel
        .queue_purge(ANALYSIS_QUEUE.into(), Default::default())
        .await
        .expect("queue should be empty before publishing the fixture");
    let mut consumer = RabbitMqJobWakeupConsumer::connect(&uri, JobKind::RepositoryIndex)
        .await
        .expect("analysis consumer should subscribe to its durable queue");

    publisher
        .publish(&wakeup)
        .await
        .expect("RabbitMQ should confirm the routed wake-up");

    let delivery = consumer
        .next()
        .await
        .expect("consumer should receive a publication")
        .expect("consumer stream should remain open");

    assert_eq!(delivery.wakeup(), &wakeup);
    assert!(delivery.is_persistent());
    delivery
        .retry()
        .await
        .expect("transient processing failures should requeue the durable wake-up");

    let redelivered = consumer
        .next()
        .await
        .expect("consumer should receive a requeued publication")
        .expect("consumer stream should remain open after requeue");

    assert_eq!(redelivered.wakeup(), &wakeup);
    assert!(redelivered.is_persistent());
    redelivered
        .ack()
        .await
        .expect("requeued test delivery should be acknowledged");
    channel.close(200, "test complete".into()).await.unwrap();
    connection.close(200, "test complete".into()).await.unwrap();
}
