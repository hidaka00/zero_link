import time
import unittest

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


if __name__ == "__main__":
    unittest.main()
