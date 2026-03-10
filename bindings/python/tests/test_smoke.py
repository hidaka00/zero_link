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
