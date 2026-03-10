from __future__ import annotations

import ctypes
import json
from dataclasses import dataclass
from enum import IntEnum
from typing import Any, Callable

from ._native import load_library
from .schemas import (
    SCHEMA_RAW_BYTES_V1,
    bool_header,
    bytes_with_mime_header,
    encode_bytes_with_mime,
    encode_bool,
    encode_float32,
    encode_float64,
    encode_int32,
    encode_int64,
    encode_string,
    encode_timestamp_ns_i64,
    float32_header,
    float64_header,
    int32_header,
    int64_header,
    string_header,
    timestamp_ns_i64_header,
    encode_uint64,
    uint64_header,
)


class ZlStatus(IntEnum):
    OK = 0
    INVALID_ARG = 1
    TIMEOUT = 2
    NOT_FOUND = 3
    BUFFER_FULL = 4
    SHM_EXHAUSTED = 5
    IPC_DISCONNECTED = 6
    INTERNAL = 255


class ZlError(RuntimeError):
    def __init__(self, code: int, op: str) -> None:
        super().__init__(
            f"{op} failed: "
            f"{ZlStatus(code).name if code in ZlStatus._value2member_map_ else code}"
        )
        self.code = code
        self.op = op


class ZlInvalidArgError(ZlError):
    pass


class ZlTimeoutError(ZlError):
    pass


class ZlNotFoundError(ZlError):
    pass


class ZlBufferFullError(ZlError):
    pass


class ZlShmExhaustedError(ZlError):
    pass


class ZlIpcDisconnectedError(ZlError):
    pass


class ZlInternalError(ZlError):
    pass


_STATUS_ERROR_MAP = {
    ZlStatus.INVALID_ARG: ZlInvalidArgError,
    ZlStatus.TIMEOUT: ZlTimeoutError,
    ZlStatus.NOT_FOUND: ZlNotFoundError,
    ZlStatus.BUFFER_FULL: ZlBufferFullError,
    ZlStatus.SHM_EXHAUSTED: ZlShmExhaustedError,
    ZlStatus.IPC_DISCONNECTED: ZlIpcDisconnectedError,
    ZlStatus.INTERNAL: ZlInternalError,
}


class _ZlClient(ctypes.Structure):
    pass


class _ZlMsgHeader(ctypes.Structure):
    _fields_ = [
        ("msg_type", ctypes.c_uint32),
        ("timestamp_ns", ctypes.c_uint64),
        ("size", ctypes.c_uint32),
        ("schema_id", ctypes.c_uint32),
        ("trace_id", ctypes.c_uint64),
    ]


class _ZlBufferRef(ctypes.Structure):
    _fields_ = [
        ("buffer_id", ctypes.c_uint64),
        ("offset", ctypes.c_uint32),
        ("length", ctypes.c_uint32),
        ("flags", ctypes.c_uint32),
    ]


@dataclass
class BufferRef:
    buffer_id: int
    offset: int
    length: int
    flags: int = 0


CallbackFn = ctypes.CFUNCTYPE(
    None,
    ctypes.c_char_p,
    ctypes.POINTER(_ZlMsgHeader),
    ctypes.c_void_p,
    ctypes.POINTER(_ZlBufferRef),
    ctypes.c_void_p,
)


@dataclass
class MsgHeader:
    msg_type: int = 2
    timestamp_ns: int = 0
    size: int = 0
    schema_id: int = SCHEMA_RAW_BYTES_V1
    trace_id: int = 1


class Client:
    def __init__(self, endpoint: str = "local") -> None:
        self._endpoint = endpoint
        self._lib = load_library()
        self._configure_signatures()
        self._client = ctypes.POINTER(_ZlClient)()
        self._callbacks: dict[str, CallbackFn] = {}

        st = self._lib.zl_client_open(endpoint.encode("utf-8"), ctypes.byref(self._client))
        self._check(st, "zl_client_open")

    def close(self) -> None:
        if bool(self._client):
            st = self._lib.zl_client_close(self._client)
            self._check(st, "zl_client_close")
            self._client = ctypes.POINTER(_ZlClient)()

    def publish(self, topic: str, payload: bytes, header: MsgHeader | None = None) -> None:
        h = header or MsgHeader(size=len(payload))
        h.size = len(payload)
        hdr = _ZlMsgHeader(
            msg_type=h.msg_type,
            timestamp_ns=h.timestamp_ns,
            size=h.size,
            schema_id=h.schema_id,
            trace_id=h.trace_id,
        )
        ptr = ctypes.c_char_p(payload)
        st = self._lib.zl_publish(
            self._client,
            topic.encode("utf-8"),
            ctypes.byref(hdr),
            ctypes.cast(ptr, ctypes.c_void_p),
            len(payload),
            None,
        )
        self._check(st, "zl_publish")

    def publish_int64(self, topic: str, value: int, *, trace_id: int = 1) -> None:
        payload = encode_int64(value)
        self.publish(topic, payload, header=int64_header(value, trace_id=trace_id))

    def publish_int32(self, topic: str, value: int, *, trace_id: int = 1) -> None:
        payload = encode_int32(value)
        self.publish(topic, payload, header=int32_header(value, trace_id=trace_id))

    def publish_uint64(self, topic: str, value: int, *, trace_id: int = 1) -> None:
        payload = encode_uint64(value)
        self.publish(topic, payload, header=uint64_header(value, trace_id=trace_id))

    def publish_float64(self, topic: str, value: float, *, trace_id: int = 1) -> None:
        payload = encode_float64(value)
        self.publish(topic, payload, header=float64_header(value, trace_id=trace_id))

    def publish_float32(self, topic: str, value: float, *, trace_id: int = 1) -> None:
        payload = encode_float32(value)
        self.publish(topic, payload, header=float32_header(value, trace_id=trace_id))

    def publish_bool(self, topic: str, value: bool, *, trace_id: int = 1) -> None:
        payload = encode_bool(value)
        self.publish(topic, payload, header=bool_header(value, trace_id=trace_id))

    def publish_timestamp_ns(self, topic: str, value: int, *, trace_id: int = 1) -> None:
        payload = encode_timestamp_ns_i64(value)
        self.publish(
            topic,
            payload,
            header=timestamp_ns_i64_header(value, trace_id=trace_id),
        )

    def publish_string(self, topic: str, value: str, *, trace_id: int = 1) -> None:
        payload = encode_string(value)
        self.publish(topic, payload, header=string_header(value, trace_id=trace_id))

    def publish_bytes_with_mime(
        self, topic: str, data: bytes, mime_type: str, *, trace_id: int = 1
    ) -> None:
        payload = encode_bytes_with_mime(data, mime_type)
        self.publish(
            topic,
            payload,
            header=bytes_with_mime_header(payload, trace_id=trace_id),
        )

    def subscribe(self, topic: str, callback: Callable[[str, MsgHeader, bytes], None]) -> None:
        def _cb(topic_ptr, header_ptr, payload_ptr, _buf_ref_ptr, _user_data):
            topic_s = ctypes.cast(topic_ptr, ctypes.c_char_p).value.decode("utf-8")
            header = MsgHeader(
                msg_type=header_ptr.contents.msg_type,
                timestamp_ns=header_ptr.contents.timestamp_ns,
                size=header_ptr.contents.size,
                schema_id=header_ptr.contents.schema_id,
                trace_id=header_ptr.contents.trace_id,
            )
            data = b""
            if payload_ptr and header.size > 0:
                data = ctypes.string_at(payload_ptr, header.size)
            callback(topic_s, header, data)

        cb = CallbackFn(_cb)
        self._callbacks[topic] = cb
        st = self._lib.zl_subscribe(self._client, topic.encode("utf-8"), cb, None)
        self._check(st, "zl_subscribe")

    def unsubscribe(self, topic: str) -> None:
        st = self._lib.zl_unsubscribe(self._client, topic.encode("utf-8"))
        self._check(st, "zl_unsubscribe")
        self._callbacks.pop(topic, None)

    def send_control(self, topic: str, command: str, payload: bytes | str = b"{}") -> None:
        if isinstance(payload, str):
            payload_bytes = payload.encode("utf-8")
        else:
            payload_bytes = payload
        payload_ptr = (
            ctypes.c_void_p()
            if not payload_bytes
            else ctypes.cast(ctypes.c_char_p(payload_bytes), ctypes.c_void_p)
        )
        st = self._lib.zl_send_control(
            self._client,
            topic.encode("utf-8"),
            command.encode("utf-8"),
            payload_ptr,
            len(payload_bytes),
        )
        self._check(st, "zl_send_control")

    def alloc_buffer(self, size: int) -> tuple[BufferRef, ctypes.Array]:
        if size <= 0:
            raise ZlInvalidArgError(int(ZlStatus.INVALID_ARG), "zl_alloc_buffer")
        out_ref = _ZlBufferRef()
        out_ptr = ctypes.c_void_p()
        st = self._lib.zl_alloc_buffer(
            self._client,
            size,
            ctypes.byref(out_ref),
            ctypes.byref(out_ptr),
        )
        self._check(st, "zl_alloc_buffer")
        if not out_ptr.value:
            raise ZlInternalError(int(ZlStatus.INTERNAL), "zl_alloc_buffer")
        data = (ctypes.c_ubyte * size).from_address(out_ptr.value)
        return (
            BufferRef(
                buffer_id=out_ref.buffer_id,
                offset=out_ref.offset,
                length=out_ref.length,
                flags=out_ref.flags,
            ),
            data,
        )

    def release_buffer(self, buffer_id: int) -> None:
        st = self._lib.zl_release_buffer(self._client, buffer_id)
        self._check(st, "zl_release_buffer")

    def publish_buffer(
        self, topic: str, payload: bytes, header: MsgHeader | None = None
    ) -> None:
        ref, data = self.alloc_buffer(len(payload))
        try:
            ctypes.memmove(ctypes.addressof(data), payload, len(payload))
            h = header or MsgHeader(msg_type=1, size=len(payload))
            h.msg_type = 1
            h.size = len(payload)
            hdr = _ZlMsgHeader(
                msg_type=h.msg_type,
                timestamp_ns=h.timestamp_ns,
                size=h.size,
                schema_id=h.schema_id,
                trace_id=h.trace_id,
            )
            native_ref = _ZlBufferRef(
                buffer_id=ref.buffer_id,
                offset=ref.offset,
                length=ref.length,
                flags=ref.flags,
            )
            st = self._lib.zl_publish(
                self._client,
                topic.encode("utf-8"),
                ctypes.byref(hdr),
                None,
                0,
                ctypes.byref(native_ref),
            )
            self._check(st, "zl_publish")
        finally:
            self.release_buffer(ref.buffer_id)

    def health(self, endpoint: str | None = None) -> dict[str, Any]:
        endpoint_value = endpoint or self._endpoint
        if not endpoint_value.startswith("daemon://"):
            raise ZlInvalidArgError(int(ZlStatus.INVALID_ARG), "zl_daemon_health")

        out_len = ctypes.c_uint32(0)
        buf = ctypes.create_string_buffer(4096)
        st = self._lib.zl_daemon_health(
            endpoint_value.encode("utf-8"),
            buf,
            len(buf),
            ctypes.byref(out_len),
        )
        self._check(st, "zl_daemon_health")
        raw = bytes(buf[: out_len.value]).decode("utf-8")
        return json.loads(raw)

    def _configure_signatures(self) -> None:
        self._lib.zl_client_open.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.POINTER(_ZlClient))]
        self._lib.zl_client_open.restype = ctypes.c_int32

        self._lib.zl_client_close.argtypes = [ctypes.POINTER(_ZlClient)]
        self._lib.zl_client_close.restype = ctypes.c_int32

        self._lib.zl_publish.argtypes = [
            ctypes.POINTER(_ZlClient),
            ctypes.c_char_p,
            ctypes.POINTER(_ZlMsgHeader),
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.POINTER(_ZlBufferRef),
        ]
        self._lib.zl_publish.restype = ctypes.c_int32

        self._lib.zl_subscribe.argtypes = [
            ctypes.POINTER(_ZlClient),
            ctypes.c_char_p,
            CallbackFn,
            ctypes.c_void_p,
        ]
        self._lib.zl_subscribe.restype = ctypes.c_int32

        self._lib.zl_unsubscribe.argtypes = [ctypes.POINTER(_ZlClient), ctypes.c_char_p]
        self._lib.zl_unsubscribe.restype = ctypes.c_int32

        self._lib.zl_alloc_buffer.argtypes = [
            ctypes.POINTER(_ZlClient),
            ctypes.c_uint32,
            ctypes.POINTER(_ZlBufferRef),
            ctypes.POINTER(ctypes.c_void_p),
        ]
        self._lib.zl_alloc_buffer.restype = ctypes.c_int32

        self._lib.zl_release_buffer.argtypes = [ctypes.POINTER(_ZlClient), ctypes.c_uint64]
        self._lib.zl_release_buffer.restype = ctypes.c_int32

        self._lib.zl_send_control.argtypes = [
            ctypes.POINTER(_ZlClient),
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_void_p,
            ctypes.c_uint32,
        ]
        self._lib.zl_send_control.restype = ctypes.c_int32

        self._lib.zl_daemon_health.argtypes = [
            ctypes.c_char_p,
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.POINTER(ctypes.c_uint32),
        ]
        self._lib.zl_daemon_health.restype = ctypes.c_int32

    @staticmethod
    def _check(status: int, op: str) -> None:
        if status != ZlStatus.OK:
            try:
                status_enum = ZlStatus(status)
            except ValueError:
                raise ZlError(status, op)
            error_type = _STATUS_ERROR_MAP.get(status_enum, ZlError)
            raise error_type(int(status_enum), op)

    def __enter__(self) -> "Client":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()
