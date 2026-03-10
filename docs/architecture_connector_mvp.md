# ZeroLink Connector MVP Architecture (Rust First)

## 1. 目的
本ドキュメントは [requirements_connector.md](./requirements_connector.md) を実装可能な設計に分解し、MVP で固定すべき技術選択を明確化する。

## 2. 設計原則（MVP）
- 初版から Rust 実装を正とする。
- 運用対象は単一ローカルPC内のプロセス間接続に限定する。
- データ面は共有メモリ参照を優先し、制御面は軽量メッセージで扱う。
- 外部公開境界は C ABI とし、言語バインディングは C ABI に追従する。
- 機能追加より先に A-01〜A-05 を満たす検証導線を優先する。

## 3. システム構成

### 3.1 コンポーネント
- `connectord`:
  - ルーティング（pub/sub）
  - 購読管理
  - バッファ参照管理
  - メトリクス収集
- Node SDK（C ABI）:
  - publish / subscribe
  - alloc_buffer / release_buffer
  - send_control
- Language Wrappers:
  - Python wrapper（C ABI 呼び出し）
  - Rust wrapper（safe facade + C ABI 互換API）

### 3.2 MVP の通信面分離
- 制御面:
  - topic 購読/解除
  - control/event 送受信
  - メタ情報連携（trace_id, schema_id）
- データ面:
  - 大容量 payload は shared memory buffer ref（buffer_id, offset, length）で連携

## 4. Rust ワークスペース構成（提案）

```text
zerolink/
  Cargo.toml
  crates/
    zl-proto/        # 共通ヘッダ、型、シリアライズ定義
    zl-ipc/          # OS 抽象 IPC（Linux/Windows）
    zl-shm/          # 共有メモリ確保/参照/リーク検知
    zl-router/       # topic 管理、配送、再接続復旧
    zl-metrics/      # latency, drop, throughput, queue depth
    zl-ffi/          # C ABI エクスポート層
    connectord/      # daemon 本体 (bin)
    zl-cli/          # 管理 CLI（topic/stat/health）
```

責務境界:
- `zl-proto`: wire format と schema_id の解釈責務
- `zl-ipc`: OS 差分の吸収責務
- `zl-ffi`: ABI 安定化責務（構造体アラインメント、戻り値規約）

## 5. IPC 実装選択（MVP 固定）

### 5.1 Linux
- 制御面: Unix Domain Socket
- データ面: `shm_open` + `mmap`

### 5.2 Windows
- 制御面: Named Pipe
- データ面: File Mapping (`CreateFileMapping` / `MapViewOfFile`)

### 5.3 OS 抽象インタフェース
`zl-ipc` が以下 trait 相当を提供する。
- `ControlChannel::send/recv`
- `SharedBuffer::create/open/release`
- `Endpoint::bind/connect`

## 6. データモデル（MVP）

### 6.1 共通ヘッダ
必須フィールド:
- `type`
- `timestamp_ns`
- `size`
- `schema_id`
- `trace_id`

### 6.2 論理メッセージ型
- `Frame`: 重データ参照を持つ
- `Event`: 軽量 payload（MVP は CBOR で固定）
- `Control`: `start/stop/reload/config`

payload エンコード方針（MVP）:
- Control/Event の payload は CBOR（RFC 8949）を採用する。
- デバッグ可読性は payload 本文ではなく、CLI の decode 表示で担保する。
- 互換性判定は `schema_id` と CBOR デコード可否の両方で行う。

### 6.3 buffer 参照
- `buffer_id`: 64-bit
- `offset`: 32-bit
- `length`: 32-bit
- `flags`: 32-bit（readonly, eos など予約）

## 7. C ABI 最小仕様（MVP v0）
実体定義ドラフトは `docs/ffi/zerolink_connector.h` を参照。

```c
// opaque handle
typedef struct zl_client zl_client_t;

typedef struct {
  uint32_t type;
  uint64_t timestamp_ns;
  uint32_t size;
  uint32_t schema_id;
  uint64_t trace_id;
} zl_msg_header_t;

typedef struct {
  uint64_t buffer_id;
  uint32_t offset;
  uint32_t length;
  uint32_t flags;
} zl_buffer_ref_t;

typedef void (*zl_subscribe_cb)(
  const char* topic,
  const zl_msg_header_t* header,
  const void* payload,
  const zl_buffer_ref_t* buf_ref,
  void* user_data
);

int32_t zl_client_open(const char* endpoint, zl_client_t** out_client);
int32_t zl_client_close(zl_client_t* client);

int32_t zl_publish(
  zl_client_t* client,
  const char* topic,
  const zl_msg_header_t* header,
  const void* payload,
  uint32_t payload_len,
  const zl_buffer_ref_t* buf_ref
);

int32_t zl_subscribe(
  zl_client_t* client,
  const char* topic,
  zl_subscribe_cb cb,
  void* user_data
);

int32_t zl_unsubscribe(zl_client_t* client, const char* topic);

int32_t zl_alloc_buffer(
  zl_client_t* client,
  uint32_t size,
  zl_buffer_ref_t* out_ref,
  void** out_ptr
);

int32_t zl_release_buffer(zl_client_t* client, uint64_t buffer_id);

int32_t zl_send_control(
  zl_client_t* client,
  const char* topic,
  const char* command,
  const void* payload,
  uint32_t payload_len
);
```

ABI 互換ポリシー:
- `v0.x` では関数追加は許容、既存関数の引数順/型変更は不可。
- 構造体拡張は末尾追加のみ。
- エラーコードは `int32_t` の固定テーブルで後方互換維持。

## 8. 受け入れ基準対応テスト計画

### 8.1 A-01 連続入力疎通
- シナリオ: dummy ASR -> connectord -> dummy Parser
- 判定: 10 分連続で無停止、欠落率閾値内

### 8.2 A-02 障害分離
- シナリオ: producer または consumer を kill
- 判定: connectord 稼働継続、残存ノード通信継続

### 8.3 A-03 参照渡し
- シナリオ: Frame payload を buffer ref で送信
- 判定: コピー回数 0〜1 回、フルコピー常態化なし

### 8.4 A-04 メトリクス取得
- シナリオ: 負荷投入時に stat API/CLI 取得
- 判定: latency p50/p95, throughput, drop, queue depth が取得可能

### 8.5 A-05 trace 追跡
- シナリオ: 2段以上パイプラインで trace_id 伝播
- 判定: trace_id で経路追跡できる

## 9. 実装マイルストーン（MVP）
- M1: `zl-proto` + `zl-ffi` の最小 API 固定
- M2: `zl-ipc`（Linux/Windows）+ `zl-router` の pub/sub 実装
- M3: `zl-shm` 導入で Frame 参照渡し実装
- M4: `zl-metrics` + `zl-cli` で観測機能追加
- M5: A-01〜A-05 の統合テスト通過

### 9.1 subscribe 通信モデルの段階方針
- 現在: daemon 経由 subscribe は pull/stream 併存。
- 既定: pull 型（`subscribe` / `poll` / `unsubscribe`）。`ZL_DAEMON_SUBSCRIBE_MODE=stream` で stream 型を有効化。
- 方針: stream を優先実装しつつ、障害時は pull へフォールバックして可用性を優先する。
- stream 型の完了条件:
  - daemon からの push 受信で callback 配信できる
  - 切断時の再接続と再購読復元が動作する
  - Linux/Windows の両方で smoke が通る

### 9.2 stream 型プロトコル草案（follow-up）
- 接続モデル: `daemon://local` で長寿命チャネルを確立し、購読単位ではなく接続単位で push を受ける。
- 最小フレーム:
  - `subscribe_open`（topic, subscription_id）
  - `message`（subscription_id, header, payload_or_buf_ref）
  - `ack`（subscription_id, seq）
  - `heartbeat`（keepalive）
  - `unsubscribe_close`（subscription_id）
- 互換方針:
  - pull 型（`subscribe/poll/unsubscribe`）は当面維持し、feature flag で stream 型と切替可能にする。
  - stream 障害時は pull 型へフォールバック可能な実装を優先する。
- 失敗時動作:
  - 接続断時は指数バックオフで再接続し、再接続成功後に active subscription を再送する。
  - 再送境界は `seq` と `trace_id` で判定し、重複配信はクライアント側で best effort 抑止する。

現状実装メモ（2026-03-10）:
- `connectord` / `zl-ffi` に試験実装を追加済み（`stream-open:<topic>` + `DaemonStreamFrame::{Heartbeat,Message}`）。
- 既定は pull 型のまま。`ZL_DAEMON_SUBSCRIBE_MODE=stream` を設定した場合のみ stream 型を有効化。
- stream 接続断時の再接続と `stream-open` 再送（再購読復元）は実装済み。
- stream 障害時の pull への自動フォールバック（再接続失敗しきい値到達時）は実装済み。
- `connectord` health に `stream_fallback_to_pull_count` と理由別カウンタ
  （`stream_fallback_connect_count` / `stream_fallback_reopen_count` / `stream_fallback_recv_count`）を追加済み。
- 残タスク: CI で意図的な切断を注入し、理由別カウンタが期待通り増える統合試験を追加。

## 10. MVP 固定値（実装開始前に確定）

### 10.1 シリアライズ方式
- D-01: Event/Control payload は CBOR で固定する。
- 理由: JSON より転送効率が高く、Bincode より他言語実装の相互運用性を取りやすい。

### 10.2 topic 命名規約
- D-02: topic は `/` 区切りの階層名とする（例: `audio/asr/text`）。
- D-03: 使用可能文字は `[a-z0-9_./-]`、先頭/末尾 `/` は禁止、連続 `/` は禁止。
- D-04: 予約プレフィックスは `_sys/`（内部制御）と `_dbg/`（デバッグ）とする。

### 10.3 共有メモリ eviction 方針
- D-05: `hard_limit_bytes` 超過時は「未参照かつ最終アクセス時刻が古い順（LRU）」で解放する。
- D-06: `pinned`（参照中）バッファは eviction 対象外とし、確保失敗時は明示エラーを返す。
- D-07: 既定値は `hard_limit_bytes=512MB`、`sweep_interval=100ms`。

### 10.4 互換試験の最小対象
- D-08: MVP 互換試験対象は C SDK / Python wrapper / Rust wrapper の 3 系統とする。
- D-09: 判定条件は「同一 topic、同一 schema_id、同一 trace_id で相互 publish/subscribe 成功」。

### 10.5 エラーコード方針（MVP 最小）
- D-10: 共通エラーコードを固定し、各 wrapper は 1:1 で変換する。
- 最小セット: `OK`, `INVALID_ARG`, `TIMEOUT`, `NOT_FOUND`, `BUFFER_FULL`, `SHM_EXHAUSTED`, `IPC_DISCONNECTED`, `INTERNAL`.
