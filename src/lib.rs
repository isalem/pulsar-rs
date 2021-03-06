#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;
#[macro_use]
extern crate nom;
#[macro_use]
extern crate prost_derive;

#[cfg(test)]
#[macro_use]
extern crate serde;

pub use client::{SerializeMessage, DeserializeMessage, Pulsar};
pub use connection::{Authentication, Connection};
pub use connection_manager::ConnectionManager;
pub use consumer::{Ack, Consumer, ConsumerBuilder, ConsumerState, Message, MultiTopicConsumer, ConsumerOptions};
pub use error::{ConnectionError, ConsumerError, Error, ProducerError, ServiceDiscoveryError};
pub use message::proto;
pub use message::proto::command_subscribe::SubType;
pub use producer::{TopicProducer, Producer, ProducerOptions};
pub use service_discovery::ServiceDiscovery;

pub mod message;
pub mod producer;
mod consumer;
mod error;
mod connection;
mod connection_manager;
mod service_discovery;
mod client;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::{Future, future, Stream};
    use futures_timer::ext::FutureExt;
    use tokio;

    use message::proto::command_subscribe::SubType;

    use crate::client::SerializeMessage;
    use crate::consumer::Message;
    use crate::Error as PulsarError;
    use crate::message::Payload;

    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestData {
        pub data: String
    }

    impl SerializeMessage for TestData {
        fn serialize_message(input: &Self) -> Result<producer::Message, PulsarError> {
            let payload = serde_json::to_vec(input)
                .map_err(|e| PulsarError::Custom(e.to_string()))?;
            Ok(producer::Message { payload, ..Default::default() })
        }
    }

    impl DeserializeMessage for TestData {
        type Output = Result<TestData, serde_json::Error>;

        fn deserialize_message(payload: Payload) -> Self::Output {
            serde_json::from_slice(&payload.data)
        }
    }

    #[derive(Debug)]
    enum Error {
        Pulsar(PulsarError),
        Message(String),
        Timeout(std::io::Error),
        Serde(serde_json::Error),
    }

    impl From<std::io::Error> for Error {
        fn from(e: std::io::Error) -> Self {
            Error::Timeout(e)
        }
    }

    impl From<PulsarError> for Error {
        fn from(e: PulsarError) -> Self {
            Error::Pulsar(e)
        }
    }

    impl From<serde_json::Error> for Error {
        fn from(e: serde_json::Error) -> Self {
            Error::Serde(e)
        }
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self {
                Error::Pulsar(e) => write!(f, "{}", e),
                Error::Message(e) => write!(f, "{}", e),
                Error::Timeout(e) => write!(f, "{}", e),
                Error::Serde(e) => write!(f, "{}", e),
            }
        }
    }

    #[test]
    #[ignore]
    fn round_trip() {
        let addr = "127.0.0.1:6650".parse().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let pulsar: Pulsar = Pulsar::new(addr, None, runtime.executor())
            .wait().unwrap();

        let producer = pulsar.producer(None);

        future::join_all((0..5000)
            .map(|_| producer.send("test", &TestData { data: "data".to_string() })))
            .map_err(|e| Error::from(e))
            .timeout(Duration::from_secs(5))
            .wait()
            .unwrap();

        let consumer: Consumer<TestData> = pulsar.consumer()
            .with_topic("test")
            .with_consumer_name("test_consumer")
            .with_subscription_type(SubType::Exclusive)
            .with_subscription("test_subscription")
            .build()
            .wait()
            .unwrap();

        consumer
            .take(5000)
            .map_err(|e| e.into())
            .for_each(move |Message { payload, ack, .. }| {
                ack.ack();
                let data = payload?;
                if data.data.as_str() == "data" {
                    Ok(())
                } else {
                    Err(Error::Message(format!("Unexpected payload: {}", &data.data)))
                }
            })
            .timeout(Duration::from_secs(5))
            .wait()
            .unwrap();
    }
}
