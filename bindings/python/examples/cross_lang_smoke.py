import argparse
import time

from zerolink import Client, SCHEMA_UTF8_STRING_V1, decode_string, string_header


def main() -> None:
    parser = argparse.ArgumentParser(description="Cross-language smoke helper")
    parser.add_argument("--endpoint", default="daemon://local")
    parser.add_argument("--mode", choices=["publish-string", "subscribe-once"], required=True)
    parser.add_argument("--topic", default="audio/asr/text")
    parser.add_argument("--message", required=True)
    parser.add_argument("--timeout-ms", type=int, default=3000)
    args = parser.parse_args()

    if args.mode == "publish-string":
        with Client(args.endpoint) as client:
            client.publish(
                args.topic,
                args.message.encode("utf-8"),
                header=string_header(args.message, trace_id=9101),
            )
        print(f"published={args.message}")
        return

    received = []
    schema_ids = []
    with Client(args.endpoint) as client:
        client.subscribe(
            args.topic,
            lambda _topic, header, payload: (
                received.append(payload),
                schema_ids.append(header.schema_id),
            ),
        )
        deadline = time.time() + (args.timeout_ms / 1000.0)
        while time.time() < deadline and not received:
            time.sleep(0.05)
        client.unsubscribe(args.topic)

    if not received:
        raise SystemExit("timeout waiting callback")
    if schema_ids[0] != SCHEMA_UTF8_STRING_V1:
        raise SystemExit(f"unexpected schema_id: {schema_ids[0]}")
    text = decode_string(received[0])
    if text != args.message:
        raise SystemExit(f"unexpected payload: {text} expected={args.message}")
    print(f"received={text}")


if __name__ == "__main__":
    main()
