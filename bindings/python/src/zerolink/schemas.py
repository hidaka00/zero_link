from __future__ import annotations

import json
import struct
from dataclasses import dataclass

SCHEMA_RAW_BYTES_V1 = 1
SCHEMA_INT64_LE_V1 = 1001
SCHEMA_FLOAT64_LE_V1 = 1002
SCHEMA_UTF8_STRING_V1 = 1003
SCHEMA_BOOL_V1 = 1004
SCHEMA_INT32_LE_V1 = 1005
SCHEMA_UINT64_LE_V1 = 1006
SCHEMA_IMAGE_FRAME_V1 = 1101


def _msg_header(*, msg_type: int, size: int, schema_id: int, trace_id: int, timestamp_ns: int):
    from .client import MsgHeader

    return MsgHeader(
        msg_type=msg_type,
        timestamp_ns=timestamp_ns,
        size=size,
        schema_id=schema_id,
        trace_id=trace_id,
    )


def int64_header(value: int, *, trace_id: int = 1, timestamp_ns: int = 0):
    return _msg_header(
        msg_type=2,
        size=8,
        schema_id=SCHEMA_INT64_LE_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def float64_header(value: float, *, trace_id: int = 1, timestamp_ns: int = 0):
    return _msg_header(
        msg_type=2,
        size=8,
        schema_id=SCHEMA_FLOAT64_LE_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def string_header(text: str, *, trace_id: int = 1, timestamp_ns: int = 0):
    encoded = text.encode("utf-8")
    return _msg_header(
        msg_type=2,
        size=len(encoded),
        schema_id=SCHEMA_UTF8_STRING_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def image_frame_header(payload_size: int, *, trace_id: int = 1, timestamp_ns: int = 0):
    return _msg_header(
        msg_type=1,
        size=payload_size,
        schema_id=SCHEMA_IMAGE_FRAME_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def bool_header(value: bool, *, trace_id: int = 1, timestamp_ns: int = 0):
    return _msg_header(
        msg_type=2,
        size=1,
        schema_id=SCHEMA_BOOL_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def int32_header(value: int, *, trace_id: int = 1, timestamp_ns: int = 0):
    return _msg_header(
        msg_type=2,
        size=4,
        schema_id=SCHEMA_INT32_LE_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def uint64_header(value: int, *, trace_id: int = 1, timestamp_ns: int = 0):
    return _msg_header(
        msg_type=2,
        size=8,
        schema_id=SCHEMA_UINT64_LE_V1,
        trace_id=trace_id,
        timestamp_ns=timestamp_ns,
    )


def encode_int64(value: int) -> bytes:
    return struct.pack("<q", value)


def decode_int64(payload: bytes) -> int:
    if len(payload) != 8:
        raise ValueError("int64 payload length must be 8")
    return struct.unpack("<q", payload)[0]


def encode_float64(value: float) -> bytes:
    return struct.pack("<d", value)


def decode_float64(payload: bytes) -> float:
    if len(payload) != 8:
        raise ValueError("float64 payload length must be 8")
    return struct.unpack("<d", payload)[0]


def encode_string(value: str) -> bytes:
    return value.encode("utf-8")


def decode_string(payload: bytes) -> str:
    return payload.decode("utf-8")


def encode_bool(value: bool) -> bytes:
    return b"\x01" if value else b"\x00"


def decode_bool(payload: bytes) -> bool:
    if len(payload) != 1:
        raise ValueError("bool payload length must be 1")
    return payload[0] != 0


def encode_int32(value: int) -> bytes:
    return struct.pack("<i", value)


def decode_int32(payload: bytes) -> int:
    if len(payload) != 4:
        raise ValueError("int32 payload length must be 4")
    return struct.unpack("<i", payload)[0]


def encode_uint64(value: int) -> bytes:
    return struct.pack("<Q", value)


def decode_uint64(payload: bytes) -> int:
    if len(payload) != 8:
        raise ValueError("uint64 payload length must be 8")
    return struct.unpack("<Q", payload)[0]


@dataclass
class ImageMeta:
    width: int
    height: int
    channels: int
    stride_bytes: int
    pixel_format: str

    def to_json_bytes(self) -> bytes:
        return json.dumps(
            {
                "width": self.width,
                "height": self.height,
                "channels": self.channels,
                "stride_bytes": self.stride_bytes,
                "pixel_format": self.pixel_format,
            },
            separators=(",", ":"),
        ).encode("utf-8")

    @classmethod
    def from_json_bytes(cls, payload: bytes) -> "ImageMeta":
        obj = json.loads(payload.decode("utf-8"))
        return cls(
            width=int(obj["width"]),
            height=int(obj["height"]),
            channels=int(obj["channels"]),
            stride_bytes=int(obj["stride_bytes"]),
            pixel_format=str(obj["pixel_format"]),
        )
