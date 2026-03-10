#ifndef ZEROLINK_CONNECTOR_H
#define ZEROLINK_CONNECTOR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * ZeroLink Connector C ABI (MVP v0)
 * - Stable surface for C/Python/Rust wrappers.
 * - Existing signatures must remain backward compatible in v0.x.
 */

/* Opaque client handle */
typedef struct zl_client zl_client_t;

/* Error code table (MVP minimum) */
typedef enum zl_status {
  ZL_OK = 0,
  ZL_INVALID_ARG = 1,
  ZL_TIMEOUT = 2,
  ZL_NOT_FOUND = 3,
  ZL_BUFFER_FULL = 4,
  ZL_SHM_EXHAUSTED = 5,
  ZL_IPC_DISCONNECTED = 6,
  ZL_INTERNAL = 255
} zl_status_t;

/* Message logical type */
typedef enum zl_msg_type {
  ZL_MSG_FRAME = 1,
  ZL_MSG_EVENT = 2,
  ZL_MSG_CONTROL = 3
} zl_msg_type_t;

/* Required common header */
typedef struct zl_msg_header {
  uint32_t type;         /* zl_msg_type_t */
  uint64_t timestamp_ns; /* monotonic or unix-ns */
  uint32_t size;         /* payload size in bytes */
  uint32_t schema_id;    /* schema compatibility id */
  uint64_t trace_id;     /* end-to-end trace key */
} zl_msg_header_t;

/* Shared-memory reference for heavy payload */
typedef struct zl_buffer_ref {
  uint64_t buffer_id;
  uint32_t offset;
  uint32_t length;
  uint32_t flags; /* reserved */
} zl_buffer_ref_t;

typedef void (*zl_subscribe_cb)(
  const char* topic,
  const zl_msg_header_t* header,
  const void* payload,
  const zl_buffer_ref_t* buf_ref,
  void* user_data
);

zl_status_t zl_client_open(const char* endpoint, zl_client_t** out_client);
zl_status_t zl_client_close(zl_client_t* client);

zl_status_t zl_publish(
  zl_client_t* client,
  const char* topic,
  const zl_msg_header_t* header,
  const void* payload,
  uint32_t payload_len,
  const zl_buffer_ref_t* buf_ref
);

zl_status_t zl_subscribe(
  zl_client_t* client,
  const char* topic,
  zl_subscribe_cb cb,
  void* user_data
);

zl_status_t zl_unsubscribe(zl_client_t* client, const char* topic);

zl_status_t zl_alloc_buffer(
  zl_client_t* client,
  uint32_t size,
  zl_buffer_ref_t* out_ref,
  void** out_ptr
);

zl_status_t zl_release_buffer(zl_client_t* client, uint64_t buffer_id);

zl_status_t zl_daemon_health(
  const char* endpoint,
  char* out_buf,
  uint32_t out_buf_len,
  uint32_t* out_written
);

zl_status_t zl_send_control(
  zl_client_t* client,
  const char* topic,
  const char* command,
  const void* payload,
  uint32_t payload_len
);

#ifdef __cplusplus
}
#endif

#endif /* ZEROLINK_CONNECTOR_H */
