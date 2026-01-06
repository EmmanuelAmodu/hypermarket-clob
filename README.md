# Hypermarket CLOB Core Engine (Rust)

Production-grade, deterministic core engine for a perps exchange on Optimism. This service is **off-chain only** and does **not** manage private keys. It consumes order/cancel/price events, maintains in-memory orderbooks, performs risk checks, matches orders, emits fills and book deltas, and writes a deterministic event log + snapshot for crash-safe replay.

## Features

- Deterministic event sourcing with WAL + snapshots.
- Sharded single-writer state machines (market_id % shard_count).
- Continuous CLOB matching + batch auction (configurable per market).
- Pre-trade risk checks (isolated margin default; cross-margin scaffolding in `risk/`).
- NATS JetStream integration behind a `Bus` trait.
- Protobuf contracts (`proto/engine.proto`, generated via `prost-build`).
- Metrics via `metrics` + Prometheus exporter, structured logs via `tracing`.
- Unit + property + simulation tests, plus a minimal benchmark.

## Repo Layout

```
src/
  bus/          # Bus trait + NATS JetStream implementation
  config/       # Config loader and structs
  engine/       # Shards + router
  matching/     # Orderbook and batch auction
  models/       # Domain types + protobuf conversions
  persistence/  # WAL + snapshot storage
  risk/         # Risk state + validation
  bin/          # engine, replay, snapshot_inspect
proto/          # protobuf schemas
config/         # example config
```

## Quick Start

### 1) Start local dependencies

```bash
docker-compose up -d
```

### 2) Run the engine

```bash
cargo run --bin engine -- --config config/example.yaml
```

### 3) Publish test messages

Example: encode `InputEvent` protobuf messages on the `clob.inputs` subject. The simplest path is a small Rust tool or a NATS CLI publisher that sends protobuf bytes.

### 4) Metrics

The Prometheus exporter is installed automatically. Scrape the `/metrics` endpoint from the running process (default exporter binding is handled by `metrics-exporter-prometheus`).

## Determinism & Replay

- All inputs are appended to the WAL **before** applying.
- Outputs (acks, fills, deltas) are appended immediately after applying.
- Snapshots include last engine sequence, checksum, and serialized state.

Replay tool:

```bash
cargo run --bin replay -- --config config/example.yaml --log ./data/engine.wal --snapshot ./data/snapshot.bin
```

Snapshot inspector:

```bash
cargo run --bin snapshot_inspect -- --snapshot ./data/snapshot.bin
```

## Tests

```bash
cargo test
```

### Benchmarks

```bash
cargo bench
```

## Notes & Simplifications

- Message bus integration uses protobuf `InputEvent`/`OutputEvent` wrappers.
- Risk engine is structured for extension (cross margin flag present), but the default is isolated margin.
- Batch auction currently uses deterministic price-time allocation at the clearing price for included orders. Orders not executed remain resting if `GTC`.
- Fee math uses integer ticks and bps; all arithmetic is integer-only for determinism.

## Config

See `config/example.yaml` for available options:
- NATS URLs and subjects
- shard_count
- market configuration (optional seed list; supports dynamic markets via NATS KV)
- WAL + snapshot paths
- snapshot interval and book delta depth

### Dynamic markets (recommended)

Markets can be created/updated at runtime by writing JSON `MarketConfig` objects into a NATS JetStream KV bucket:
- Bucket: `bus.markets_bucket` (default `MARKETS`)
- Key: `<market_id>`
- Value: JSON-encoded `MarketConfig` (same fields as in `config/example.yaml`)
