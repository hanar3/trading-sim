# Trading workspace

React 19 + TypeScript + Vite, TanStack Query, shadcn/ui components, and Tailwind CSS.
The Rust Cargo workspace remains at the repository root; `web` is an npm workspace.

## Run

From the repository root (Node 24, Rust, protoc, and Docker are required):

```sh
npm ci
npm run dev -- --seed
```

Open http://127.0.0.1:5173. The runner builds the Rust services, starts RabbitMQ,
creates a fresh queue/database session under `.local/`, and starts all three services
and Vite. `--seed` explicitly places 24 real simulated resting orders and one
crossing buy through the HTTP API. Omit it for an empty book. Ctrl-C stops the
owned services; session data/logs and RabbitMQ are retained.

Ports default to engine 4000, gateway 3000, and frontend 5173. Override with
`ENGINE_PORT`, `API_PORT`, and `WEB_PORT`. Occupied ports fail with an explanation.
To work against services you already started, use `npm run dev:web`; set `API_URL`
if the gateway is not at http://127.0.0.1:3000.

Click a book price to populate the limit-price field. Choose buy/sell, enter a
quantity and simulated participant ID, and submit. Unmatched orders appear under
Open orders; matching orders appear in Recent trades. Cancellation applies to the
remaining quantity. This is a local simulation with no authentication or balances.

## Market-data contract and performance

- `GET /market`: a full JSON snapshot from gateway memory.
- `GET /market/stream`: SSE `book` events carrying full replacement snapshots,
  plus ten-second comment heartbeats. EventSource reconnects automatically.
- Up to 20 price levels per side, the latest 100 open orders, and 40 recent fills.
  Counts and cumulative depth refer to the current projection; depth totals are
  cumulative across the displayed levels.
- All monetary quantities, prices, user/order IDs, and sequence numbers are decimal
  strings. Request amounts also use strings; conversion uses BigInt throughout.
- HTTP `202` means queued, not executed. The UI never optimistically edits the book.
  Changes appear only after committed engine events reach the projection.

The frontend adds **no book locks, snapshot commands, or new engine events**.
Order tracking carries a UUID in each command; the matching loop moves it into the
existing acceptance event. All progress computation runs outside that thread. One background gateway reader tails the existing SQLite
`processed_events` log by indexed rowid, irrespective of browser count. It applies
accepted orders, fills, and cancellations to its own indexed projection. At most
once per 100 ms it serializes a bounded snapshot, only when data/status changed.
Snapshots are shared as immutable bytes; a latest-value watch channel coalesces
updates for slow clients instead of backpressuring matching.

The reader uses one read-only connection, pages of 2,048 events, and at most eight
pages per tick. It yields between pages. The persistor uses SQLite WAL so ordinary
reads and writes can coexist. There are still CPU, disk, and network costs outside
the engine; this is architectural isolation, not a claim of measured zero system
impact. Core matching benchmarks retain their previous measurement boundaries.

The view is **eventually consistent**: its delay includes broker delivery,
persistence, and up to a normal 100 ms refresh interval. During backlog it reports
`syncing`; unavailable data reports `unavailable`. Acceptance and fills are separate
events/transactions, so an intermediate event prefix may briefly include an order
whose fills have not arrived yet. It is not an atomic engine snapshot.

Gateway restarts rebuild from the committed event log. Use the same absolute
`APP_DATABASE__FILE` for gateway and persistor. Start each new engine session with
a fresh database and queue; the runner does this automatically. Do not truncate,
vacuum/rebuild, replace, or prune the receipt log while a gateway is tailing it.
The feed status describes the gateway's read-model connection, not engine/persistor
health. A stopped persistor cannot be distinguished from an idle market by this
read-only feed alone.

For a custom engine pair, set matching gateway metadata through
`APP_MARKET__BASE_CURRENCY`, `APP_MARKET__QUOTE_CURRENCY`, `APP_MARKET__BASE_SCALE`,
and `APP_MARKET__QUOTE_SCALE` (defaults BTC/USD and scales 8/2). Keep these consistent
with the single engine's configuration; the demo seed assumes BTC/USD.

Vite proxies `/api/*` to the gateway without browser CORS changes. `npm run build`
produces static files in `web/dist`; a production host must route `/api/*` to the
gateway and disable response buffering on the SSE route. No public hosting or
authentication is configured by this local demo.

## Sticky order progress

Every submitted order gets a persistent, manually dismissible activity card. It
shows submitting/queued, open, partially filled, filled, or cancelled progress.
Expand the card for matched trades, the order reference, remaining quantity, and
an action to cancel the remainder. A cancelled partially filled order keeps its
actual fill percentage; cancellation does not count as execution. Cards remain
until closed during the current page session, including after completion. Reloading
the page starts a fresh activity list. Dismissing a card does not cancel its order.

`POST /orders` now returns `{ "client_order_id": "<uuid>" }` with HTTP 202. The
engine echoes this gateway-generated ID in `OrderAccepted`, and the persistor
stores it. `GET /orders/status/{client_order_id}?details=false` reads committed
acceptance, aggregate fills, and cancellation in one read-only transaction. This
is independent of the bounded market snapshot, so fully filled orders and old
fills do not disappear from tracking when they leave the live book/tape.

Active cards poll every 750 ms while the page is visible; terminal cards stop
interval polling. Expanding sets `details=true` to fetch up to 100 recent matched
trades. Summaries include every fill even when details are capped. Queries use
indexes on correlation IDs and both trade order-ID columns, with a separate
read-only pool capped at two connections; they do not query or lock the engine.
Work still scales with tracked cards and the number of fills per order.

Until persisted acceptance exists, the card remains queued/awaiting confirmation.
It does not guess that an unobserved order filled, failed, or timed out. Lost HTTP
responses are marked unconfirmed, since an order may already have been accepted.
The engine still has no correlated rejection event for commands rejected after
HTTP enqueueing. Broker/database delays also delay displayed progress. If updates
fail, the card keeps its last confirmed numbers and retries. New protobuf fields
and the additive migration require rebuilding/restarting all Rust services together.

## Verification

```sh
npm test
npm run build
npm run format:check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
python3 scripts/smoke_test.py
```

The smoke test checks snapshots, gateway reconstruction, exact IDs above 2^53, and
live SSE updates after placement/cancellation in addition to the original matching
and persistence checks. Rust unit tests cover partial-fill aggregation, level
removal, bounded snapshot ordering, and slow-subscriber coalescing. Frontend tests
cover exact integer conversion, formatting, invalid values, sticky tracking,
expanded matched trades, completion, cancellation, and manual dismissal. CI runs both
workspaces. A browser pass also exercised buy/sell fills, a resting order,
cancellation, invalid input, and phone/desktop layouts.
