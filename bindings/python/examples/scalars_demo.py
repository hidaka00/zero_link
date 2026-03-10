import time

from zerolink import Client, decode_float64, decode_int64, decode_string


def main() -> None:
    topic = "audio/asr/text"
    received = []

    def on_msg(_topic, header, payload):
        if header.schema_id == 1001:
            value = decode_int64(payload)
            kind = "int64"
        elif header.schema_id == 1002:
            value = decode_float64(payload)
            kind = "float64"
        elif header.schema_id == 1003:
            value = decode_string(payload)
            kind = "string"
        else:
            value = payload
            kind = f"unknown({header.schema_id})"
        print(f"kind={kind} trace_id={header.trace_id} value={value}")
        received.append(kind)

    with Client("local") as client:
        client.subscribe(topic, on_msg)
        client.publish_int64(topic, 42, trace_id=101)
        client.publish_float64(topic, 3.5, trace_id=102)
        client.publish_string(topic, "hello-scalar", trace_id=103)

        deadline = time.time() + 2.0
        while time.time() < deadline and len(received) < 3:
            time.sleep(0.05)
        client.unsubscribe(topic)

    if len(received) < 3:
        raise SystemExit("did not receive all scalar messages")


if __name__ == "__main__":
    main()
