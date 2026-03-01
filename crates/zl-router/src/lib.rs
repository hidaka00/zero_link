use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use zl_proto::{BufferRef, MessageHeader};

#[derive(Debug)]
pub enum RouterError {
    InvalidTopic,
    Poisoned,
}

#[derive(Debug, Clone)]
pub struct RoutedMessage {
    pub header: MessageHeader,
    pub payload: Vec<u8>,
    pub buffer_ref: Option<BufferRef>,
}

#[derive(Debug, Default)]
struct RouterInner {
    subscribers: HashMap<String, Vec<Sender<RoutedMessage>>>,
}

#[derive(Debug, Default, Clone)]
pub struct Router {
    inner: Arc<Mutex<RouterInner>>,
}

pub fn validate_topic(topic: &str) -> Result<(), RouterError> {
    if topic.is_empty() || topic.starts_with('/') || topic.ends_with('/') || topic.contains("//") {
        return Err(RouterError::InvalidTopic);
    }

    if !topic
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-'))
    {
        return Err(RouterError::InvalidTopic);
    }

    Ok(())
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self, topic: &str) -> Result<Receiver<RoutedMessage>, RouterError> {
        validate_topic(topic)?;
        let (tx, rx) = mpsc::channel();
        let mut inner = self.inner.lock().map_err(|_| RouterError::Poisoned)?;
        inner
            .subscribers
            .entry(topic.to_string())
            .or_default()
            .push(tx);
        Ok(rx)
    }

    pub fn publish(&self, topic: &str, msg: RoutedMessage) -> Result<usize, RouterError> {
        validate_topic(topic)?;
        let mut inner = self.inner.lock().map_err(|_| RouterError::Poisoned)?;
        let Some(subscribers) = inner.subscribers.get_mut(topic) else {
            return Ok(0);
        };

        let mut delivered = 0usize;
        subscribers.retain(|tx| {
            if tx.send(msg.clone()).is_ok() {
                delivered += 1;
                true
            } else {
                false
            }
        });

        Ok(delivered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_topic_accepts_expected_format() {
        assert!(validate_topic("audio/asr/text").is_ok());
        assert!(validate_topic("_dbg/pipeline-1").is_ok());
        assert!(validate_topic("a.b/c_d-0").is_ok());
    }

    #[test]
    fn validate_topic_rejects_invalid_format() {
        assert!(validate_topic("").is_err());
        assert!(validate_topic("/audio").is_err());
        assert!(validate_topic("audio/").is_err());
        assert!(validate_topic("audio//text").is_err());
        assert!(validate_topic("Audio/text").is_err());
    }

    #[test]
    fn router_delivers_published_message_to_subscriber() {
        let router = Router::new();
        let rx = router.subscribe("audio/asr/text").expect("subscribe should succeed");
        let sent = RoutedMessage {
            header: MessageHeader {
                msg_type: 2,
                timestamp_ns: 10,
                size: 5,
                schema_id: 1,
                trace_id: 42,
            },
            payload: b"hello".to_vec(),
            buffer_ref: None,
        };

        let delivered = router
            .publish("audio/asr/text", sent.clone())
            .expect("publish should succeed");

        assert_eq!(delivered, 1);
        let recv = rx.recv().expect("should receive message");
        assert_eq!(recv.header.trace_id, sent.header.trace_id);
        assert_eq!(recv.payload, sent.payload);
    }
}
