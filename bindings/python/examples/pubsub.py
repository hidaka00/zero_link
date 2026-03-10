import time

from zerolink import Client, SCHEMA_UTF8_STRING_V1, string_header


def main() -> None:
    got = []
    got_schema_ids = []

    def on_msg(topic, header, payload):
        print(f"topic={topic} trace_id={header.trace_id} payload={payload.decode('utf-8')}")
        got.append(payload)
        got_schema_ids.append(header.schema_id)

    with Client("local") as client:
        client.subscribe("audio/asr/text", on_msg)
        msg = "hello-from-python"
        client.publish("audio/asr/text", msg.encode("utf-8"), header=string_header(msg))
        deadline = time.time() + 2.0
        while time.time() < deadline and not got:
            time.sleep(0.05)
        client.unsubscribe("audio/asr/text")

    if not got:
        raise SystemExit("no callback received")
    if got_schema_ids != [SCHEMA_UTF8_STRING_V1]:
        raise SystemExit(f"unexpected schema_ids: {got_schema_ids}")


if __name__ == "__main__":
    main()
