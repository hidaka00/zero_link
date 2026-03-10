import unittest

from zerolink import (
    ImageMeta,
    SCHEMA_FLOAT64_LE_V1,
    SCHEMA_IMAGE_FRAME_V1,
    SCHEMA_INT64_LE_V1,
    SCHEMA_UTF8_STRING_V1,
    decode_float64,
    decode_int64,
    decode_string,
    encode_float64,
    encode_int64,
    encode_string,
    float64_header,
    image_frame_header,
    int64_header,
    string_header,
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
