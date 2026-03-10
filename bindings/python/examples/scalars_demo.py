import time

from zerolink import Client, decode_float64, decode_int64, decode_string
from zerolink import (
    decode_bool,
    decode_int32,
    decode_uint64,
    SCHEMA_BOOL_V1,
    SCHEMA_FLOAT64_LE_V1,
    SCHEMA_INT32_LE_V1,
    SCHEMA_INT64_LE_V1,
    SCHEMA_UINT64_LE_V1,
    SCHEMA_UTF8_STRING_V1,
)


def main() -> None:
    topic = "audio/asr/text"
    received = []

    def on_msg(_topic, header, payload):
        if header.schema_id == SCHEMA_INT64_LE_V1:
            value = decode_int64(payload)
            kind = "int64"
        elif header.schema_id == SCHEMA_INT32_LE_V1:
            value = decode_int32(payload)
            kind = "int32"
        elif header.schema_id == SCHEMA_UINT64_LE_V1:
            value = decode_uint64(payload)
            kind = "uint64"
        elif header.schema_id == SCHEMA_FLOAT64_LE_V1:
            value = decode_float64(payload)
            kind = "float64"
        elif header.schema_id == SCHEMA_BOOL_V1:
            value = decode_bool(payload)
            kind = "bool"
        elif header.schema_id == SCHEMA_UTF8_STRING_V1:
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
        client.publish_int32(topic, -7, trace_id=102)
        client.publish_uint64(topic, 1234567890123, trace_id=103)
        client.publish_float64(topic, 3.5, trace_id=104)
        client.publish_bool(topic, True, trace_id=105)
        client.publish_string(topic, "hello-scalar", trace_id=106)

        deadline = time.time() + 2.0
        while time.time() < deadline and len(received) < 6:
            time.sleep(0.05)
        client.unsubscribe(topic)

    if len(received) < 6:
        raise SystemExit("did not receive all scalar messages")


if __name__ == "__main__":
    main()
