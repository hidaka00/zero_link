import time
import unittest
import os

from zerolink import (
    Client,
    ZlBufferFullError,
    ZlError,
    ZlInternalError,
    ZlInvalidArgError,
    ZlIpcDisconnectedError,
    ZlNotFoundError,
    ZlShmExhaustedError,
    ZlStatus,
    ZlTimeoutError,
    SCHEMA_UTF8_STRING_V1,
    SCHEMA_INT64_LE_V1,
    SCHEMA_FLOAT64_LE_V1,
    SCHEMA_BOOL_V1,
    SCHEMA_INT32_LE_V1,
    SCHEMA_UINT64_LE_V1,
    decode_bool,
    decode_int32,
    decode_int64,
    decode_float64,
    decode_string,
    decode_uint64,
    string_header,
)


class SmokeTest(unittest.TestCase):
    def test_pubsub_roundtrip_local(self) -> None:
        received = []
        schema_ids = []

        def on_msg(_topic, header, payload):
            received.append(payload)
            schema_ids.append(header.schema_id)

        with Client("local") as client:
            client.subscribe("audio/asr/text", on_msg)
            text = "py-smoke"
            client.publish(
                "audio/asr/text",
                text.encode("utf-8"),
                header=string_header(text),
            )
            deadline = time.time() + 2.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(received, [b"py-smoke"])
        self.assertEqual(schema_ids, [SCHEMA_UTF8_STRING_V1])

    def test_send_control_roundtrip_local(self) -> None:
        received = []

        def on_msg(_topic, _header, payload):
            received.append(payload)

        with Client("local") as client:
            client.subscribe("_sys/control", on_msg)
            client.send_control("_sys/control", "reload", "{}")
            deadline = time.time() + 2.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("_sys/control")

        self.assertEqual(len(received), 1)
        self.assertGreater(len(received[0]), 0)

    def test_publish_scalar_helpers_local(self) -> None:
        received = []
        headers = []

        def on_msg(_topic, header, payload):
            headers.append(header)
            received.append(payload)

        with Client("local") as client:
            client.subscribe("audio/asr/text", on_msg)
            client.publish_int64("audio/asr/text", 42, trace_id=101)
            client.publish_int32("audio/asr/text", -7, trace_id=104)
            client.publish_uint64("audio/asr/text", 1234567890123, trace_id=105)
            client.publish_float64("audio/asr/text", 3.5, trace_id=102)
            client.publish_bool("audio/asr/text", True, trace_id=106)
            client.publish_string("audio/asr/text", "scalar", trace_id=103)
            deadline = time.time() + 2.0
            while time.time() < deadline and len(received) < 6:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(len(received), 6)
        self.assertEqual(headers[0].schema_id, SCHEMA_INT64_LE_V1)
        self.assertEqual(headers[1].schema_id, SCHEMA_INT32_LE_V1)
        self.assertEqual(headers[2].schema_id, SCHEMA_UINT64_LE_V1)
        self.assertEqual(headers[3].schema_id, SCHEMA_FLOAT64_LE_V1)
        self.assertEqual(headers[4].schema_id, SCHEMA_BOOL_V1)
        self.assertEqual(headers[5].schema_id, SCHEMA_UTF8_STRING_V1)
        self.assertEqual(decode_int64(received[0]), 42)
        self.assertEqual(decode_int32(received[1]), -7)
        self.assertEqual(decode_uint64(received[2]), 1234567890123)
        self.assertAlmostEqual(decode_float64(received[3]), 3.5)
        self.assertEqual(decode_bool(received[4]), True)
        self.assertEqual(decode_string(received[5]), "scalar")

    def test_publish_buffer_roundtrip_local(self) -> None:
        received = []

        def on_msg(_topic, _header, payload):
            received.append(payload)

        with Client("local") as client:
            client.subscribe("audio/asr/text", on_msg)
            client.publish_buffer("audio/asr/text", b"py-buffer")
            deadline = time.time() + 2.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(received, [b"py-buffer"])

    def test_pubsub_roundtrip_daemon(self) -> None:
        if os.getenv("ZEROLINK_PY_SMOKE_DAEMON", "0") not in {"1", "true", "TRUE"}:
            self.skipTest("set ZEROLINK_PY_SMOKE_DAEMON=1 to enable daemon smoke")

        endpoint = os.getenv("ZEROLINK_PY_ENDPOINT", "daemon://local")
        received = []
        schema_ids = []

        def on_msg(_topic, header, payload):
            received.append(payload)
            schema_ids.append(header.schema_id)

        with Client(endpoint) as client:
            client.subscribe("audio/asr/text", on_msg)
            text = "py-daemon-smoke"
            client.publish(
                "audio/asr/text",
                text.encode("utf-8"),
                header=string_header(text),
            )
            health = client.health()
            deadline = time.time() + 3.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(received, [b"py-daemon-smoke"])
        self.assertEqual(schema_ids, [SCHEMA_UTF8_STRING_V1])
        self.assertEqual(health.get("status"), "ok")
        self.assertEqual(health.get("service"), "connectord")

    def test_publish_buffer_roundtrip_daemon(self) -> None:
        if os.getenv("ZEROLINK_PY_SMOKE_DAEMON", "0") not in {"1", "true", "TRUE"}:
            self.skipTest("set ZEROLINK_PY_SMOKE_DAEMON=1 to enable daemon smoke")

        endpoint = os.getenv("ZEROLINK_PY_ENDPOINT", "daemon://local")
        received = []

        def on_msg(_topic, _header, payload):
            received.append(payload)

        with Client(endpoint) as client:
            client.subscribe("audio/asr/text", on_msg)
            client.publish_buffer("audio/asr/text", b"py-daemon-buffer")
            deadline = time.time() + 3.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(received, [b"py-daemon-buffer"])

    def test_health_requires_daemon_endpoint(self) -> None:
        with Client("local") as client:
            with self.assertRaises(ZlInvalidArgError):
                client.health()


class ErrorMappingTest(unittest.TestCase):
    def test_status_to_exception_mapping(self) -> None:
        cases = [
            (ZlStatus.INVALID_ARG, ZlInvalidArgError),
            (ZlStatus.TIMEOUT, ZlTimeoutError),
            (ZlStatus.NOT_FOUND, ZlNotFoundError),
            (ZlStatus.BUFFER_FULL, ZlBufferFullError),
            (ZlStatus.SHM_EXHAUSTED, ZlShmExhaustedError),
            (ZlStatus.IPC_DISCONNECTED, ZlIpcDisconnectedError),
            (ZlStatus.INTERNAL, ZlInternalError),
        ]
        for status, error_type in cases:
            with self.assertRaises(error_type):
                Client._check(int(status), "unit-test")

    def test_unknown_status_falls_back_to_base_error(self) -> None:
        with self.assertRaises(ZlError):
            Client._check(999, "unit-test")


if __name__ == "__main__":
    unittest.main()
