import time

from zerolink import (
    Client,
    SCHEMA_RAW_BYTES_V1,
    SCHEMA_UTF8_STRING_V1,
    string_header,
)


def main() -> None:
    endpoint = "daemon://local"
    got = []
    got_schema_ids = []

    def on_msg(topic, header, payload):
        print(
            "topic="
            f"{topic} trace_id={header.trace_id} schema_id={header.schema_id} "
            f"payload={payload.decode('utf-8')}"
        )
        got.append(payload)
        got_schema_ids.append(header.schema_id)

    with Client(endpoint) as client:
        health = client.health()
        print(f"health.status={health.get('status')} service={health.get('service')}")

        client.subscribe("audio/asr/text", on_msg)
        client.publish_buffer("audio/asr/text", b"daemon-buffer-from-python")
        text = "daemon-text-from-python"
        client.publish("audio/asr/text", text.encode("utf-8"), header=string_header(text))

        client.send_control("_sys/control", "reload", "{}")

        deadline = time.time() + 3.0
        while time.time() < deadline and len(got) < 2:
            time.sleep(0.05)
        client.unsubscribe("audio/asr/text")

    if len(got) < 2:
        raise SystemExit("did not receive expected callbacks from daemon flow")
    expected = {SCHEMA_RAW_BYTES_V1, SCHEMA_UTF8_STRING_V1}
    if set(got_schema_ids) != expected:
        raise SystemExit(f"unexpected schema_ids: {got_schema_ids}")


if __name__ == "__main__":
    main()
