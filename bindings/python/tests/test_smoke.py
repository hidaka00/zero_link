import time
import unittest
import os

from zerolink import Client


class SmokeTest(unittest.TestCase):
    def test_pubsub_roundtrip_local(self) -> None:
        received = []

        def on_msg(_topic, _header, payload):
            received.append(payload)

        with Client("local") as client:
            client.subscribe("audio/asr/text", on_msg)
            client.publish("audio/asr/text", b"py-smoke")
            deadline = time.time() + 2.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(received, [b"py-smoke"])

    def test_pubsub_roundtrip_daemon(self) -> None:
        if os.getenv("ZEROLINK_PY_SMOKE_DAEMON", "0") not in {"1", "true", "TRUE"}:
            self.skipTest("set ZEROLINK_PY_SMOKE_DAEMON=1 to enable daemon smoke")

        endpoint = os.getenv("ZEROLINK_PY_ENDPOINT", "daemon://local")
        received = []

        def on_msg(_topic, _header, payload):
            received.append(payload)

        with Client(endpoint) as client:
            client.subscribe("audio/asr/text", on_msg)
            client.publish("audio/asr/text", b"py-daemon-smoke")
            deadline = time.time() + 3.0
            while time.time() < deadline and not received:
                time.sleep(0.05)
            client.unsubscribe("audio/asr/text")

        self.assertEqual(received, [b"py-daemon-smoke"])


if __name__ == "__main__":
    unittest.main()
