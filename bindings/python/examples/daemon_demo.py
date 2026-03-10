import time

from zerolink import Client


def main() -> None:
    endpoint = "daemon://local"
    got = []

    def on_msg(topic, header, payload):
        print(f"topic={topic} trace_id={header.trace_id} payload={payload.decode('utf-8')}")
        got.append(payload)

    with Client(endpoint) as client:
        health = client.health()
        print(f"health.status={health.get('status')} service={health.get('service')}")

        client.subscribe("audio/asr/text", on_msg)
        client.publish_buffer("audio/asr/text", b"daemon-buffer-from-python")

        client.send_control("_sys/control", "reload", "{}")

        deadline = time.time() + 3.0
        while time.time() < deadline and not got:
            time.sleep(0.05)
        client.unsubscribe("audio/asr/text")

    if not got:
        raise SystemExit("no callback received from daemon flow")


if __name__ == "__main__":
    main()
