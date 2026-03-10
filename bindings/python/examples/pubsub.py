import time

from zerolink import Client


def main() -> None:
    got = []

    def on_msg(topic, header, payload):
        print(f"topic={topic} trace_id={header.trace_id} payload={payload.decode('utf-8')}")
        got.append(payload)

    with Client("local") as client:
        client.subscribe("audio/asr/text", on_msg)
        client.publish("audio/asr/text", b"hello-from-python")
        deadline = time.time() + 2.0
        while time.time() < deadline and not got:
            time.sleep(0.05)
        client.unsubscribe("audio/asr/text")

    if not got:
        raise SystemExit("no callback received")


if __name__ == "__main__":
    main()
