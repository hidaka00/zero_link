use std::time::{SystemTime, UNIX_EPOCH};

use zl_proto::MessageHeader;
use zl_router::{RoutedMessage, Router};

fn main() {
    let router = Router::new();
    let topic = "audio/asr/text";
    let rx = router
        .subscribe(topic)
        .expect("valid topic must be subscribable");

    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    let msg = RoutedMessage {
        header: MessageHeader {
            msg_type: 2,
            timestamp_ns: now_ns,
            size: 5,
            schema_id: 1,
            trace_id: 1001,
        },
        payload: b"hello".to_vec(),
        buffer_ref: None,
    };

    let delivered = router.publish(topic, msg).expect("publish should succeed");
    println!("connectord: delivered={delivered}");

    if let Ok(received) = rx.recv() {
        println!(
            "connectord: recv trace_id={} payload_len={}",
            received.header.trace_id,
            received.payload.len()
        );
    }
}
