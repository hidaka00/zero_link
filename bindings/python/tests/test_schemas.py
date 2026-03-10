import unittest

from zerolink import (
    ImageMeta,
    MimeBytesEnvelope,
    SCHEMA_BOOL_V1,
    SCHEMA_BYTES_WITH_MIME_JSON_V1,
    SCHEMA_FLOAT32_LE_V1,
    SCHEMA_FLOAT64_LE_V1,
    SCHEMA_IMAGE_FRAME_V1,
    SCHEMA_INT32_LE_V1,
    SCHEMA_INT64_LE_V1,
    SCHEMA_TIMESTAMP_NS_I64_V1,
    SCHEMA_UINT64_LE_V1,
    SCHEMA_UTF8_STRING_V1,
    bool_header,
    bytes_with_mime_header,
    decode_bool,
    decode_bytes_with_mime,
    decode_float32,
    decode_int32,
    decode_float64,
    decode_int64,
    decode_string,
    decode_timestamp_ns_i64,
    decode_uint64,
    encode_bool,
    encode_bytes_with_mime,
    encode_float32,
    encode_int32,
    encode_float64,
    encode_int64,
    encode_string,
    encode_timestamp_ns_i64,
    encode_uint64,
    int32_header,
    float32_header,
    float64_header,
    image_frame_header,
    int64_header,
    string_header,
    timestamp_ns_i64_header,
    uint64_header,
)


class SchemaCodecTest(unittest.TestCase):
    def test_int64_codec(self) -> None:
        v = -123456789
        encoded = encode_int64(v)
        self.assertEqual(len(encoded), 8)
        self.assertEqual(decode_int64(encoded), v)
        header = int64_header(v, trace_id=9)
        self.assertEqual(header.schema_id, SCHEMA_INT64_LE_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 8)
        self.assertEqual(header.trace_id, 9)

    def test_float64_codec(self) -> None:
        v = 3.1415926
        encoded = encode_float64(v)
        self.assertEqual(len(encoded), 8)
        self.assertAlmostEqual(decode_float64(encoded), v)
        header = float64_header(v)
        self.assertEqual(header.schema_id, SCHEMA_FLOAT64_LE_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 8)

    def test_string_codec(self) -> None:
        s = "zero-link"
        encoded = encode_string(s)
        self.assertEqual(decode_string(encoded), s)
        header = string_header(s)
        self.assertEqual(header.schema_id, SCHEMA_UTF8_STRING_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, len(encoded))

    def test_bool_codec(self) -> None:
        encoded_true = encode_bool(True)
        encoded_false = encode_bool(False)
        self.assertEqual(decode_bool(encoded_true), True)
        self.assertEqual(decode_bool(encoded_false), False)
        header = bool_header(True)
        self.assertEqual(header.schema_id, SCHEMA_BOOL_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 1)

    def test_int32_codec(self) -> None:
        v = -12345
        encoded = encode_int32(v)
        self.assertEqual(len(encoded), 4)
        self.assertEqual(decode_int32(encoded), v)
        header = int32_header(v)
        self.assertEqual(header.schema_id, SCHEMA_INT32_LE_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 4)

    def test_uint64_codec(self) -> None:
        v = 2**40 + 123
        encoded = encode_uint64(v)
        self.assertEqual(len(encoded), 8)
        self.assertEqual(decode_uint64(encoded), v)
        header = uint64_header(v)
        self.assertEqual(header.schema_id, SCHEMA_UINT64_LE_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 8)

    def test_float32_codec(self) -> None:
        v = 1.25
        encoded = encode_float32(v)
        self.assertEqual(len(encoded), 4)
        self.assertAlmostEqual(decode_float32(encoded), v, places=6)
        header = float32_header(v)
        self.assertEqual(header.schema_id, SCHEMA_FLOAT32_LE_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 4)

    def test_timestamp_ns_i64_codec(self) -> None:
        v = 1_700_000_000_000_000_123
        encoded = encode_timestamp_ns_i64(v)
        self.assertEqual(len(encoded), 8)
        self.assertEqual(decode_timestamp_ns_i64(encoded), v)
        header = timestamp_ns_i64_header(v)
        self.assertEqual(header.schema_id, SCHEMA_TIMESTAMP_NS_I64_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, 8)

    def test_bytes_with_mime_codec(self) -> None:
        mime_type = "application/octet-stream"
        payload = b"\x00\x01demo\xff"
        encoded = encode_bytes_with_mime(payload, mime_type)
        decoded_mime, decoded_payload = decode_bytes_with_mime(encoded)
        self.assertEqual(decoded_mime, mime_type)
        self.assertEqual(decoded_payload, payload)

        env = MimeBytesEnvelope.from_json_bytes(encoded)
        self.assertEqual(env.mime_type, mime_type)
        self.assertEqual(env.data, payload)
        self.assertEqual(MimeBytesEnvelope(mime_type, payload).to_json_bytes(), encoded)

        header = bytes_with_mime_header(encoded)
        self.assertEqual(header.schema_id, SCHEMA_BYTES_WITH_MIME_JSON_V1)
        self.assertEqual(header.msg_type, 2)
        self.assertEqual(header.size, len(encoded))

    def test_image_meta_codec(self) -> None:
        meta = ImageMeta(
            width=640,
            height=480,
            channels=3,
            stride_bytes=640 * 3,
            pixel_format="rgb8",
        )
        encoded = meta.to_json_bytes()
        decoded = ImageMeta.from_json_bytes(encoded)
        self.assertEqual(decoded, meta)
        header = image_frame_header(640 * 480 * 3)
        self.assertEqual(header.schema_id, SCHEMA_IMAGE_FRAME_V1)
        self.assertEqual(header.msg_type, 1)


if __name__ == "__main__":
    unittest.main()
