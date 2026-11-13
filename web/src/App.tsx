import { useCallback, useEffect, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import {
  Activity,
  ArrowDownLeft,
  ArrowRight,
  ArrowUpRight,
  Check,
  ChevronRight,
  CircleHelp,
  Layers3,
  Loader2,
  Radio,
  RefreshCw,
  Terminal,
  X,
  Zap,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  cancelOrder,
  depthWidth,
  formatUnits,
  parseUnits,
  participantId,
  submitOrder,
  type Level,
  type Market,
  type Snapshot,
} from "@/lib/market";
import { useMarket } from "@/lib/use-market";
import { OrderTracker, type TrackedOrder } from "@/components/order-tracker";
import {
  CommandError,
  type OrderCommand,
  type OrderReceipt,
} from "@/lib/market";
import { cn } from "@/lib/utils";

type Notice = { id: number; title: string; detail?: string; error?: boolean };
const fallback: Market = {
  base_currency: "BTC",
  quote_currency: "USD",
  base_scale: 8,
  quote_scale: 2,
};

export default function App() {
  const feed = useMarket();
  const snapshot = feed.data;
  const market = snapshot?.market ?? fallback;
  const [price, setPrice] = useState("");
  const [trackedOrders, setTrackedOrders] = useState<TrackedOrder[]>([]);
  const trackOrder = (order: OrderCommand, market: Market) => {
    const id = crypto.randomUUID();
    setTrackedOrders((current) => [
      ...current,
      { id, order, market, submittedAt: Date.now(), phase: "submitting" },
    ]);
    return id;
  };
  const updateTracked = (id: string, patch: Partial<TrackedOrder>) =>
    setTrackedOrders((current) =>
      current.map((order) =>
        order.id === id ? { ...order, ...patch } : order,
      ),
    );
  const dismissTracked = (id: string) =>
    setTrackedOrders((current) => current.filter((order) => order.id !== id));
  const [notices, setNotices] = useState<Notice[]>([]);
  const notify = (title: string, detail?: string, error = false) =>
    setNotices((current) => [
      ...current.slice(-2),
      { id: Date.now() + Math.random(), title, detail, error },
    ]);
  const dismissNotice = useCallback(
    (id: number) =>
      setNotices((current) => current.filter((notice) => notice.id !== id)),
    [],
  );
  const bid = snapshot?.bids[0]?.price,
    ask = snapshot?.asks[0]?.price;
  const last = snapshot?.trades[0];
  const spread =
    bid && ask ? (BigInt(ask) - BigInt(bid)).toString() : undefined;

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Workspace">
        <a href="/" className="brand" aria-label="Trading Sim home">
          <span className="brand-mark">
            <Activity size={22} strokeWidth={2.5} />
          </span>
          <span>
            trading<span className="brand-light">sim</span>
            <span className="brand-dot">.</span>
          </span>
        </a>
        <div className="workspace-label">
          WORKSPACE <span>01</span>
        </div>
        <a href="#market" className="nav-item active">
          <Layers3 size={17} /> Spot trading <ChevronRight size={14} />
        </a>
        <div className="sidebar-note">
          <span className="tiny-square" />
          <span>LOCAL ENVIRONMENT</span>
        </div>
        <div className="sidebar-bottom">
          <div className="engine-symbol">
            <Terminal size={18} />
          </div>
          <div>
            <strong>Rust matching engine</strong>
            <p>Simulation workspace</p>
          </div>
          <span className="version">v0.1</span>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            Workspace <ChevronRight size={13} />
            <strong>Spot trading</strong>
          </div>
          <div className="topbar-right">
            <Badge className="simulation-badge">SIMULATION</Badge>
            <a className="help-link" href="#how-it-works">
              <CircleHelp size={16} />
              <span>How it works</span>
            </a>
          </div>
        </header>
        <main id="market">
          <div className="page-heading">
            <div>
              <div className="eyebrow">THE TRADING WORKSPACE</div>
              <h1>A clear view of the market.</h1>
              <p>Place a limit order. Watch the book move.</p>
            </div>
            <div
              className={cn("feed-status", feed.live ? "online" : "offline")}
            >
              <span className="status-dot" />
              <div>
                <strong>
                  {feed.live
                    ? "Market feed connected"
                    : feed.isError
                      ? "Gateway unavailable"
                      : feed.connection === "reconnecting"
                        ? "Reconnecting to feed"
                        : snapshot?.status === "unavailable"
                          ? "Waiting for persistence"
                          : "Connecting to market"}
                </strong>
                <span>
                  {feed.live
                    ? "Live updates · up to 10 / sec"
                    : "Updates resume automatically"}
                </span>
              </div>
              <Radio size={17} />
            </div>
          </div>

          {(feed.isError || (snapshot && !feed.live)) && (
            <div className="connection-banner" role="status">
              <Activity size={16} />
              <span>
                {feed.isError
                  ? feed.error.message
                  : snapshot?.status === "unavailable"
                    ? "The read model is unavailable. Start the persistor and check the shared database path."
                    : feed.connection === "reconnecting"
                      ? "The feed disconnected. Displayed prices may be stale while reconnecting."
                      : "Replaying persisted events. Order entry will be ready when the feed catches up."}
              </span>
              {feed.isError && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void feed.refetch()}
                >
                  <RefreshCw />
                  Retry
                </Button>
              )}
            </div>
          )}

          <section className="market-summary" aria-label="Market overview">
            <div className="pair-heading">
              <div className="coin-icon">
                {market.base_currency === "BTC"
                  ? "₿"
                  : market.base_currency.slice(0, 1)}
              </div>
              <div>
                <h2>
                  {market.base_currency}
                  <span> / {market.quote_currency}</span>
                  <Badge className="spot-badge">SPOT</Badge>
                </h2>
                <p>
                  {market.base_currency === "BTC"
                    ? "Bitcoin"
                    : market.base_currency}{" "}
                  · Limit order market
                </p>
              </div>
            </div>
            <div className="summary-divider" />
            <Metric
              label="Last trade"
              value={last ? formatUnits(last.price, market.quote_scale) : "—"}
              suffix={market.quote_currency}
            />
            <Metric
              label="Best bid"
              value={bid ? formatUnits(bid, market.quote_scale) : "—"}
              tone="buy-text"
            />
            <Metric
              label="Best ask"
              value={ask ? formatUnits(ask, market.quote_scale) : "—"}
              tone="sell-text"
            />
            <Metric
              label="Spread"
              value={spread ? formatUnits(spread, market.quote_scale) : "—"}
              suffix={spread ? market.quote_currency : undefined}
            />
          </section>

          <div className="trading-grid">
            <Card
              className={cn("book-card", snapshot && !feed.live && "stale")}
            >
              <CardHeader>
                <div className="card-title-group">
                  <CardTitle>Order book</CardTitle>
                  <Badge className="quiet-badge">20 levels</Badge>
                </div>
                <span className="small-label">
                  <span className="mini-dot" /> Aggregated depth
                </span>
              </CardHeader>
              <div className="book-sides">
                <BookSide
                  side="buy"
                  levels={snapshot?.bids ?? []}
                  market={market}
                  loading={feed.isPending}
                  onPrice={setPrice}
                />
                <BookSide
                  side="sell"
                  levels={snapshot?.asks ?? []}
                  market={market}
                  loading={feed.isPending}
                  onPrice={setPrice}
                />
              </div>
              <div className="book-footer">
                <span>Click a price to use it in your order</span>
                <span className="mono">
                  {snapshot?.open_order_count ?? 0} open orders
                </span>
              </div>
            </Card>
            <OrderEntry
              market={market}
              price={price}
              setPrice={setPrice}
              live={feed.live}
              trackOrder={trackOrder}
              updateTracked={updateTracked}
            />
          </div>

          <div className="activity-grid">
            <OpenOrders
              snapshot={snapshot}
              market={market}
              live={feed.live}
              notify={notify}
            />
            <Card className="trades-card">
              <CardHeader>
                <CardTitle>Recent trades</CardTitle>
                <span className="small-label">Last 40 fills</span>
              </CardHeader>
              <div className="tape-head">
                <span>Price ({market.quote_currency})</span>
                <span>Quantity ({market.base_currency})</span>
                <span>Orders</span>
              </div>
              <div className="tape-body">
                {snapshot?.trades.length ? (
                  snapshot.trades.map((trade) => (
                    <div className="tape-row" key={trade.event_id}>
                      <span
                        className={cn(
                          "mono tape-price",
                          trade.side === 1 ? "buy-text" : "sell-text",
                        )}
                      >
                        {trade.side === 1 ? (
                          <ArrowUpRight size={13} />
                        ) : (
                          <ArrowDownLeft size={13} />
                        )}
                        {formatUnits(trade.price, market.quote_scale)}
                      </span>
                      <span className="mono">
                        {formatUnits(trade.quantity, market.base_scale)}
                      </span>
                      <span className="mono muted">
                        #{trade.maker_order_id} → #{trade.taker_order_id}
                      </span>
                    </div>
                  ))
                ) : (
                  <EmptyState
                    icon={<ArrowRight size={20} />}
                    title="The next match starts here"
                    detail="Fills appear when a buy and sell order cross."
                  />
                )}
              </div>
            </Card>
          </div>

          <footer id="how-it-works" className="page-footer">
            <div>
              <Zap size={14} />
              <span>
                Price priority. Time priority. Every order in its place.
              </span>
            </div>
            <p>
              Orders are simulated. “Queued” confirms submission, not execution.
            </p>
          </footer>
        </main>
      </div>
      <div className="toast-stack" role="region" aria-label="Order activity">
        {trackedOrders
          .slice()
          .reverse()
          .map((order) => (
            <OrderTracker
              key={order.id}
              tracked={order}
              dismiss={dismissTracked}
            />
          ))}
        {notices.map((notice) => (
          <Toast key={notice.id} notice={notice} dismiss={dismissNotice} />
        ))}
      </div>
    </div>
  );
}

function Metric({
  label,
  value,
  suffix,
  tone,
}: {
  label: string;
  value: string;
  suffix?: string;
  tone?: string;
}) {
  return (
    <div className="metric">
      <span>{label}</span>
      <div className={cn("mono", tone)}>
        {value}
        {suffix && <small>{suffix}</small>}
      </div>
    </div>
  );
}
function BookSide({
  side,
  levels,
  market,
  loading,
  onPrice,
}: {
  side: "buy" | "sell";
  levels: Level[];
  market: Market;
  loading: boolean;
  onPrice: (price: string) => void;
}) {
  let cumulative = 0n;
  const rows = levels.map((level) => ({
    ...level,
    total: (cumulative += BigInt(level.quantity)).toString(),
  }));
  const max = cumulative;
  return (
    <div className={cn("book-side", side)}>
      <div className="side-heading">
        <span>
          <span className="side-dot" />
          {side === "buy" ? "Bids" : "Asks"}
        </span>
        <span>{side === "buy" ? "BUY" : "SELL"}</span>
      </div>
      <div className="book-columns">
        <span>Price ({market.quote_currency})</span>
        <span>Qty ({market.base_currency})</span>
        <span>Total ({market.base_currency})</span>
      </div>
      <div className="book-rows">
        {rows.length ? (
          rows.map((row) => (
            <button
              key={row.price}
              className="depth-row"
              onClick={() =>
                onPrice(formatUnits(row.price, market.quote_scale, false))
              }
              title={`Use ${formatUnits(row.price, market.quote_scale)} as limit price`}
            >
              <span
                className="depth-bar"
                style={{ width: `${depthWidth(row.total, max)}%` }}
              />
              <span className="depth-price mono">
                {formatUnits(row.price, market.quote_scale)}
              </span>
              <span className="mono">
                {formatUnits(row.quantity, market.base_scale)}
              </span>
              <span className="mono depth-total">
                {formatUnits(row.total, market.base_scale)}
              </span>
            </button>
          ))
        ) : (
          <div className="book-empty">
            {loading ? (
              <Loader2 className="animate-spin" size={20} />
            ) : (
              <Layers3 size={23} strokeWidth={1.3} />
            )}
            <strong>
              {loading
                ? "Connecting to the book"
                : `No ${side === "buy" ? "bids" : "asks"} yet`}
            </strong>
            <span>
              {loading
                ? "Your live market will appear here."
                : `Place a ${side === "buy" ? "buy" : "sell"} order to add liquidity.`}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
function OrderEntry({
  market,
  price,
  setPrice,
  live,
  trackOrder,
  updateTracked,
}: {
  market: Market;
  price: string;
  setPrice: (price: string) => void;
  live: boolean;
  trackOrder: (order: OrderCommand, market: Market) => string;
  updateTracked: (id: string, patch: Partial<TrackedOrder>) => void;
}) {
  const [side, setSide] = useState(1);
  const [quantity, setQuantity] = useState("0.001");
  const [user, setUser] = useState("1");
  const [error, setError] = useState("");
  const mutation = useMutation<OrderReceipt, Error, OrderCommand, string>({
    mutationFn: submitOrder,
    onMutate: (order) => trackOrder(order, market),
    onSuccess: (receipt, _order, id) => {
      setError("");
      if (id)
        updateTracked(id, {
          clientId: receipt.client_order_id,
          phase: "queued",
        });
    },
    onError: (err, _order, id) => {
      setError(err.message);
      if (id)
        updateTracked(id, {
          phase:
            err instanceof CommandError && err.definite
              ? "failed"
              : "unconfirmed",
          error: err.message,
        });
    },
  });
  let total = "—";
  try {
    const notional =
      (BigInt(parseUnits(price, market.quote_scale)) *
        BigInt(parseUnits(quantity, market.base_scale))) /
      10n ** BigInt(market.base_scale);
    total = formatUnits(notional.toString(), market.quote_scale);
  } catch {
    /* Incomplete input has no notional yet. */
  }
  function submit(event: React.FormEvent) {
    event.preventDefault();
    try {
      const order = {
        user_id: participantId(user),
        side,
        price: parseUnits(price, market.quote_scale),
        quantity: parseUnits(quantity, market.base_scale),
        base_currency: market.base_currency,
        quote_currency: market.quote_currency,
      };
      setError("");
      mutation.mutate(order);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Check your order values.");
    }
  }
  return (
    <Card className="order-card">
      <CardHeader>
        <CardTitle>Place an order</CardTitle>
        <span className="small-label">LIMIT</span>
      </CardHeader>
      <CardContent>
        <div className="side-tabs" role="group" aria-label="Order side">
          <button
            className={cn(side === 1 && "selected buy-selected")}
            aria-pressed={side === 1}
            onClick={() => setSide(1)}
          >
            <ArrowDownLeft size={15} /> Buy {market.base_currency}
          </button>
          <button
            className={cn(side === 2 && "selected sell-selected")}
            aria-pressed={side === 2}
            onClick={() => setSide(2)}
          >
            <ArrowUpRight size={15} /> Sell {market.base_currency}
          </button>
        </div>
        <form onSubmit={submit} noValidate>
          <div className="field">
            <label htmlFor="price">Limit price</label>
            <div className="input-with-unit">
              <Input
                id="price"
                value={price}
                onChange={(event) => setPrice(event.target.value)}
                placeholder="0.00"
                inputMode="decimal"
                autoComplete="off"
                aria-describedby={error ? "order-error" : undefined}
              />
              <span>{market.quote_currency}</span>
            </div>
          </div>
          <div className="field">
            <label htmlFor="quantity">Quantity</label>
            <div className="input-with-unit">
              <Input
                id="quantity"
                value={quantity}
                onChange={(event) => setQuantity(event.target.value)}
                placeholder="0.00000000"
                inputMode="decimal"
                autoComplete="off"
              />
              <span>{market.base_currency}</span>
            </div>
            <div className="quantity-presets">
              {["0.001", "0.01", "0.1"].map((amount) => (
                <button
                  type="button"
                  key={amount}
                  onClick={() => setQuantity(amount)}
                >
                  {amount} {market.base_currency}
                </button>
              ))}
            </div>
          </div>
          <div className="field participant-field">
            <label htmlFor="participant">
              Participant ID <span>Simulated account</span>
            </label>
            <Input
              id="participant"
              value={user}
              onChange={(event) => setUser(event.target.value)}
              inputMode="numeric"
              autoComplete="off"
            />
          </div>
          <div className="order-notional">
            <span>Order value</span>
            <strong className="mono">
              {total}
              <small>{market.quote_currency}</small>
            </strong>
          </div>
          {error && (
            <p id="order-error" className="form-error" role="alert">
              {error}
            </p>
          )}
          <Button
            type="submit"
            disabled={!live || mutation.isPending}
            className={cn(
              "submit-order",
              side === 1 ? "buy-button" : "sell-button",
            )}
          >
            {mutation.isPending ? (
              <Loader2 className="animate-spin" />
            ) : side === 1 ? (
              <ArrowDownLeft />
            ) : (
              <ArrowUpRight />
            )}
            {mutation.isPending
              ? "Submitting…"
              : `Place ${side === 1 ? "buy" : "sell"} limit order`}
          </Button>
          <p className="order-hint">
            {!live
              ? "Waiting for a live market feed."
              : "Crossing orders match. Unfilled quantity rests in the book."}
          </p>
        </form>
      </CardContent>
    </Card>
  );
}
function OpenOrders({
  snapshot,
  market,
  live,
  notify,
}: {
  snapshot?: Snapshot;
  market: Market;
  live: boolean;
  notify: (title: string, detail?: string, error?: boolean) => void;
}) {
  const cancel = useMutation({
    mutationFn: cancelOrder,
    onSuccess: (_, id) =>
      notify(
        `Cancellation for #${id} queued`,
        "The order disappears when cancellation is persisted.",
      ),
    onError: (error: Error) =>
      notify("Cancellation failed", error.message, true),
  });
  return (
    <Card className="orders-card">
      <CardHeader>
        <div className="card-title-group">
          <CardTitle>Open orders</CardTitle>
          <Badge className="count-badge">
            {snapshot?.open_order_count ?? 0}
          </Badge>
        </div>
        <span className="small-label">All participants</span>
      </CardHeader>
      <div className="orders-scroll">
        <table className="orders-table">
          <thead>
            <tr>
              <th>Order / participant</th>
              <th>Side</th>
              <th>Limit price</th>
              <th>Remaining / quantity</th>
              <th>
                <span className="sr-only">Action</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {snapshot?.open_orders.map((order) => (
              <tr key={order.order_id}>
                <td>
                  <span className="mono">#{order.order_id}</span>
                  <small>Participant {order.user_id}</small>
                </td>
                <td>
                  <span
                    className={cn(
                      "side-pill",
                      order.side === 1 ? "buy-pill" : "sell-pill",
                    )}
                  >
                    {order.side === 1 ? "Buy" : "Sell"}
                  </span>
                </td>
                <td className="mono">
                  {formatUnits(order.price, market.quote_scale)}
                </td>
                <td className="mono">
                  {formatUnits(order.remaining, market.base_scale)}
                  <small>
                    / {formatUnits(order.quantity, market.base_scale)}
                  </small>
                </td>
                <td>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={!live || cancel.isPending}
                    onClick={() => cancel.mutate(order.order_id)}
                    aria-label={`Cancel order ${order.order_id}`}
                  >
                    {cancel.isPending && cancel.variables === order.order_id ? (
                      <Loader2 className="animate-spin" />
                    ) : (
                      <X size={13} />
                    )}
                    Cancel
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!snapshot?.open_orders.length && (
          <EmptyState
            icon={<Layers3 size={22} />}
            title="A clean slate"
            detail="Your resting limit orders will appear here."
          />
        )}
      </div>
      {snapshot && snapshot.open_order_count > 100 && (
        <p className="orders-cap">Showing the latest 100 open orders.</p>
      )}
    </Card>
  );
}
function EmptyState({
  icon,
  title,
  detail,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
}) {
  return (
    <div className="empty-state">
      <span>{icon}</span>
      <strong>{title}</strong>
      <p>{detail}</p>
    </div>
  );
}
function Toast({
  notice,
  dismiss,
}: {
  notice: Notice;
  dismiss: (id: number) => void;
}) {
  useEffect(() => {
    const timer = setTimeout(() => dismiss(notice.id), 6500);
    return () => clearTimeout(timer);
  }, [dismiss, notice.id]);
  return (
    <div className={cn("toast", notice.error && "error-toast")}>
      <span className="toast-icon">
        {notice.error ? <X size={17} /> : <Check size={17} />}
      </span>
      <div>
        <strong>{notice.title}</strong>
        <p>{notice.detail}</p>
      </div>
      <button
        aria-label="Dismiss notification"
        onClick={() => dismiss(notice.id)}
      >
        <X size={14} />
      </button>
    </div>
  );
}
