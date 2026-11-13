# Local development

## React trading workspace

With Node 24 installed, run `npm ci` and `npm run dev -- --seed` from the root.
Open http://127.0.0.1:5173. This starts the Rust services, RabbitMQ, and React with
fresh session data and real simulated liquidity. Omit `--seed` for an empty book.
See [web/README.md](web/README.md) for details.

## Setup and checks

Install Rust 1.92 or newer, Protocol Buffers (`brew install protobuf` on macOS),
Python 3, and Docker Desktop/Engine. Start Docker, then run from the repository root:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
docker compose up -d
python3 scripts/smoke_test.py
```

The demo uses local RabbitMQ at port 5672 with `admin` / `pass`, and its management
API at port 15672 for duplicate-event replay and cleanup. Wait for RabbitMQ to
finish starting before invoking it. It uses unique queue names, temporary SQLite
files, and available ports, leaving existing development data alone.

## Interactive services

Use a fresh database and a new queue for each engine session. From the repository
root, set these values in **each** of three terminals, then run one service per terminal:

```sh
export APP_AMQP__CHANNEL=orders-demo-1
export APP_DATABASE__FILE="$PWD/message-persistor/demo-1.sqlite"
# Terminal 1:
cargo run --locked -p engine
# Terminal 2:
cargo run --locked -p message-persistor
# Terminal 3:
cargo run --locked -p api-gateway
```

Configuration is loaded relative to each crate, so all commands can run from the
repository root. The explicit absolute database path above avoids dependence on
working directory. The persistor creates the file and applies versioned migrations.
Both AMQP services create the queue. No SQLx CLI or compile-time database is needed.
To only initialize/migrate a database, run `./init_db.sh`; it honors
`APP_DATABASE__FILE` and otherwise uses `message-persistor/db.sqlite`.

Use the same absolute `APP_DATABASE__FILE` for gateway and persistor: the gateway
reads committed events to reconstruct and stream the order book. WAL mode allows
its read-only connection to coexist with persistence writes.

The gateway defaults to 127.0.0.1:3000 and the engine to 127.0.0.1:4000. Override
`APP_APPLICATION__PORT` per process and `APP_ENGINE__PORT` on the gateway when
needed. The engine supports its configured pair only (BTC/USD by default).

## Place and match limit orders

```sh
curl --fail -i http://127.0.0.1:3000/orders \
  -H 'Content-Type: application/json' \
  -d '{"user_id":1,"side":2,"price":10000,"quantity":100000,"base_currency":"BTC","quote_currency":"USD"}'
curl --fail -i http://127.0.0.1:3000/orders \
  -H 'Content-Type: application/json' \
  -d '{"user_id":2,"side":1,"price":10000,"quantity":100000,"base_currency":"BTC","quote_currency":"USD"}'
```

Side 1 is buy; side 2 is sell. With the configured integer scales these represent
0.001 BTC at USD 100.00 per BTC. On a fresh engine, sell order 1 is maker and buy
order 2 is taker. Both return `202 Accepted`, acknowledging only gateway enqueueing.
Matching acceptance and persistence must be checked in logs and SQLite.

To cancel a resting order, first place it at a non-crossing price, then send its
engine-assigned ID (visible in engine logs):

```sh
curl --fail -i -X DELETE http://127.0.0.1:3000/orders \
  -H 'Content-Type: application/json' -d '{"order_id":3}'
```

The persistor retains the original accepted quantity and price. `orders.cancelled`
records cancellation; `trades` records fills and their execution prices. The orders
table is an acceptance ledger, not a live remaining-quantity order book.

## Failure and restart boundaries

- Engine state and queued commands/events are volatile. HTTP `202` does not imply
  matching, durable acceptance, or execution. There are no correlated rejection
  responses or order-status endpoint yet. Unknown currency pairs are rejected by
  the engine after gateway enqueueing.
- Order IDs restart at 1 with a fresh engine. Start each engine session with a new
  database **and** queue, as the automated demo does. Restarting the persistor alone
  is supported; restarting the engine against previous session data is not.
- Publisher failure stops the engine to avoid continuing silently without event
  delivery. Buffered events can still be lost. Confirms do not replace a durable
  engine journal; automatic reconnect/replay is not implemented.
- The persistor retries transient database failures on the current event to
  preserve ordering. Permanent invalid events are logged and negatively acknowledged
  without requeueing. There is no dead-letter queue yet.
- Each event must have a nonempty AMQP message ID. Legacy events without IDs are
  rejected. Event receipts are retained indefinitely for deduplication.
- Cancellation uses lazy queue removal. Cancelled orders never execute, but their
  queue entries can remain until matching visits that level.
- Per-message channels and frame sizes are bounded. Connection count and connection
  lifetime are not yet bounded; this is a local experimental service.

## Existing databases

Versioned databases created with SQLx migrations upgrade automatically. Historical
trades receive a NULL execution price because it was never recorded. Previously
swapped maker/taker records are preserved; this migration does not infer repairs.

The database created manually during the initial investigation has no SQLx
migration history. Keep it as a historical snapshot and select a new database for
current runs. Do not run migrations blindly over an unversioned existing schema.
The demo always uses a fresh temporary database.

Stop services with Ctrl-C. Stop RabbitMQ with `docker compose stop` when finished.
The SQLite files and benchmark output are ignored by Git.
