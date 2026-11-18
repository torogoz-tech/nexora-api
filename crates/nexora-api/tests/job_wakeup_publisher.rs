use futures_util::StreamExt;
use lapin::{
    Connection, ConnectionProperties,
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions},
    types::FieldTable,
};
use nexora_api::{ANALYSIS_QUEUE, RabbitMqJobWakeupPublisher};
use nexora_platform::{JobKind, JobWakeup};

#[tokio::test]
#[ignore = "requires RabbitMQ; run against the test server to avoid a broker service in GitHub Actions"]
async fn publishes_a_confirmed_persistent_analysis_wakeup() {
    let uri = std::env::var("NEXORA_TEST_RABBITMQ_URL")
        .expect("NEXORA_TEST_RABBITMQ_URL must point to a reachable RabbitMQ test broker");
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
    channel
        .basic_qos(1, BasicQosOptions::default())
        .await
        .expect("consumer should limit unacknowledged wake-ups");
    let mut consumer = channel
        .basic_consume(
            ANALYSIS_QUEUE.into(),
            "".into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("test consumer should subscribe to the analysis queue");

    publisher
        .publish(&wakeup)
        .await
        .expect("RabbitMQ should confirm the routed wake-up");

    let delivery = consumer
        .next()
        .await
        .expect("consumer stream should remain open")
        .expect("consumer should receive a publication");

    assert_eq!(
        serde_json::from_slice::<JobWakeup>(&delivery.data).unwrap(),
        wakeup
    );
    assert_eq!(*delivery.properties.delivery_mode(), Some(2));
    delivery
        .nack(BasicNackOptions {
            requeue: true,
            ..BasicNackOptions::default()
        })
        .await
        .expect("transient processing failures should requeue the durable wake-up");

    let redelivered = consumer
        .next()
        .await
        .expect("consumer stream should remain open after requeue")
        .expect("consumer should receive a requeued publication");

    assert_eq!(
        serde_json::from_slice::<JobWakeup>(&redelivered.data).unwrap(),
        wakeup
    );
    assert_eq!(*redelivered.properties.delivery_mode(), Some(2));
    redelivered
        .ack(BasicAckOptions::default())
        .await
        .expect("requeued test delivery should be acknowledged");
    channel.close(200, "test complete".into()).await.unwrap();
    connection.close(200, "test complete".into()).await.unwrap();
}
