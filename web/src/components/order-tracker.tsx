import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Check,
  ChevronDown,
  Loader2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  cancelOrder,
  fillPercent,
  formatUnits,
  getOrderProgress,
  isTerminal,
  type Market,
  type OrderCommand,
} from "@/lib/market";
import { cn } from "@/lib/utils";

export interface TrackedOrder {
  id: string;
  clientId?: string;
  order: OrderCommand;
  market: Market;
  submittedAt: number;
  phase: "submitting" | "queued" | "failed" | "unconfirmed";
  error?: string;
}

export function OrderTracker({
  tracked,
  dismiss,
}: {
  tracked: TrackedOrder;
  dismiss: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const client = useQueryClient();
  const progress = useQuery({
    queryKey: ["order-progress", tracked.clientId],
    queryFn: ({ signal }) =>
      getOrderProgress(tracked.clientId!, expanded, signal),
    enabled: !!tracked.clientId,
    staleTime: 500,
    retry: 1,
    refetchInterval: (query) => (isTerminal(query.state.data) ? false : 750),
    refetchIntervalInBackground: false,
  });
  // Keep one cache entry per order so a failed details request retains confirmed totals.
  useEffect(() => {
    if (expanded && tracked.clientId) void progress.refetch();
  }, [expanded, tracked.clientId, progress.refetch]);
  const cancel = useMutation({
    mutationFn: cancelOrder,
    onSuccess: () =>
      client.invalidateQueries({
        queryKey: ["order-progress", tracked.clientId],
      }),
  });
  const data = progress.data;
  const filled = data?.filled ?? "0";
  const percentage = fillPercent(filled, tracked.order.quantity);
  const terminal = isTerminal(data);
  const state =
    tracked.phase === "failed"
      ? "Not queued"
      : tracked.phase === "unconfirmed"
        ? "Submission unconfirmed"
        : tracked.phase === "submitting"
          ? "Submitting"
          : data?.status === "filled"
            ? "Filled"
            : data?.status === "cancelled"
              ? "Cancelled"
              : data?.status === "partially_filled"
                ? "Partially filled"
                : data?.status === "open"
                  ? "Open"
                  : "Queued";
  const buying = tracked.order.side === 1;
  const label = `${buying ? "Buy" : "Sell"} ${formatUnits(tracked.order.quantity, tracked.market.base_scale)} ${tracked.market.base_currency}`;
  const pending =
    !terminal && tracked.phase !== "failed" && tracked.phase !== "unconfirmed";
  const detailsId = `order-details-${tracked.id}`;

  return (
    <section
      className={cn(
        "order-tracker",
        terminal && "tracker-terminal",
        tracked.phase === "failed" && "tracker-failed",
      )}
      aria-label={`Order progress: ${label}`}
    >
      <div className="tracker-header">
        <span className={cn("tracker-icon", buying ? "buy-text" : "sell-text")}>
          {data?.status === "filled" ? (
            <Check size={17} />
          ) : buying ? (
            <ArrowDownLeft size={17} />
          ) : (
            <ArrowUpRight size={17} />
          )}
        </span>
        <div className="tracker-heading">
          <strong>{label}</strong>
          <span>
            Limit {formatUnits(tracked.order.price, tracked.market.quote_scale)}{" "}
            {tracked.market.quote_currency}
            {data?.order_id && ` · Order #${data.order_id}`}
          </span>
        </div>
        <button
          className="tracker-dismiss"
          title="Dismiss tracker without cancelling the order"
          aria-label={`Dismiss ${data?.order_id ? `order ${data.order_id}` : label} tracker`}
          onClick={() => dismiss(tracked.id)}
        >
          <X size={15} />
        </button>
      </div>
      <div className="tracker-status-row">
        <span
          className={cn(
            "tracker-state",
            data?.status === "filled" && "tracker-filled",
          )}
          role="status"
        >
          {pending && !data?.order_id && (
            <Loader2 size={12} className="animate-spin" />
          )}
          {state}
        </span>
        <span className="mono tracker-percentage">
          {percentage.toFixed(2)}%
        </span>
      </div>
      <div
        className={cn(
          "tracker-progress",
          pending && !data?.order_id && "tracker-waiting",
        )}
        role="progressbar"
        aria-label="Order filled"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percentage}
        aria-valuetext={`${percentage.toFixed(2)}% filled`}
      >
        <span style={{ width: `${percentage}%` }} />
      </div>
      <div className="tracker-quantity">
        <span>
          <strong className="mono">
            {formatUnits(filled, tracked.market.base_scale)}
          </strong>{" "}
          / {formatUnits(tracked.order.quantity, tracked.market.base_scale)}{" "}
          {tracked.market.base_currency} filled
        </span>
        <span>
          {data?.fill_count ?? 0}{" "}
          {(data?.fill_count ?? 0) === 1 ? "fill" : "fills"}
        </span>
      </div>
      {data?.status === "cancelled" && (
        <p className="tracker-note">
          {formatUnits(
            data.cancelled_quantity ?? "0",
            tracked.market.base_scale,
          )}{" "}
          {tracked.market.base_currency} cancelled. Executed fills are retained.
        </p>
      )}
      {progress.isError && (
        <p className="tracker-note tracker-warning">
          Updates paused. Retrying automatically; the last confirmed progress is
          shown.
        </p>
      )}
      {tracked.phase === "queued" &&
        (!data || data.status === "awaiting_acceptance") && (
          <p className="tracker-note">
            Waiting for persisted acceptance. Queued does not confirm execution.
          </p>
        )}
      {tracked.error && (
        <p className="tracker-note tracker-warning">
          {tracked.error}
          {tracked.phase === "unconfirmed" &&
            " The order may have been accepted; do not automatically resubmit."}
        </p>
      )}
      <button
        className="tracker-expand"
        aria-expanded={expanded}
        aria-controls={detailsId}
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? "Hide details" : "Show details"}
        <ChevronDown
          size={14}
          className={expanded ? "tracker-chevron-expanded" : ""}
        />
      </button>
      {expanded && (
        <div id={detailsId} className="tracker-details">
          <dl>
            <div>
              <dt>Submitted</dt>
              <dd>{new Date(tracked.submittedAt).toLocaleTimeString()}</dd>
            </div>
            <div>
              <dt>Participant</dt>
              <dd className="mono">{tracked.order.user_id}</dd>
            </div>
            <div>
              <dt>Remaining open</dt>
              <dd className="mono">
                {data?.remaining !== undefined
                  ? `${formatUnits(data.remaining, tracked.market.base_scale)} ${tracked.market.base_currency}`
                  : "Awaiting acceptance"}
              </dd>
            </div>
            {tracked.clientId && (
              <div>
                <dt>Reference</dt>
                <dd className="tracker-reference mono" title={tracked.clientId}>
                  {tracked.clientId}
                </dd>
              </div>
            )}
          </dl>
          <div className="tracker-fills-heading">
            Matched trades <span>{data?.fill_count ?? 0}</span>
          </div>
          {data?.trades?.length ? (
            <ol className="tracker-fills">
              {data.trades.map((fill) => (
                <li key={fill.trade_id}>
                  <div>
                    <strong>Trade #{fill.trade_id}</strong>
                    <span>
                      {fill.maker_order_id === data.order_id
                        ? "Maker"
                        : "Taker"}{" "}
                      · Matched #
                      {fill.maker_order_id === data.order_id
                        ? fill.taker_order_id
                        : fill.maker_order_id}
                    </span>
                  </div>
                  <div className="mono">
                    <strong>
                      {formatUnits(fill.quantity, tracked.market.base_scale)}{" "}
                      {tracked.market.base_currency}
                    </strong>
                    <span>
                      @{" "}
                      {fill.price === null
                        ? "Unknown price"
                        : `${formatUnits(fill.price, tracked.market.quote_scale)} ${tracked.market.quote_currency}`}
                    </span>
                  </div>
                </li>
              ))}
            </ol>
          ) : (
            <p className="tracker-no-fills">
              {(data?.fill_count ?? 0) > 0
                ? progress.isError
                  ? "Matched trade details are temporarily unavailable."
                  : "Loading matched trades…"
                : "No confirmed fills yet."}
            </p>
          )}
          {(data?.fill_count ?? 0) > 100 && (
            <p className="tracker-note">
              Showing the latest 100 trades of {data?.fill_count}. Progress
              includes every fill.
            </p>
          )}
          {data?.order_id && !terminal && (
            <Button
              variant="outline"
              size="sm"
              className="tracker-cancel"
              disabled={cancel.isPending}
              onClick={() => cancel.mutate(data.order_id!)}
            >
              {cancel.isPending ? (
                <Loader2 size={12} className="animate-spin" />
              ) : (
                <X size={12} />
              )}
              Cancel remaining quantity
            </Button>
          )}
          {cancel.isSuccess && !terminal && (
            <p className="tracker-note">
              Cancellation queued; awaiting confirmation.
            </p>
          )}
          {cancel.isError && (
            <p className="tracker-note tracker-warning">
              Cancellation could not be confirmed. Check the order before
              retrying.
            </p>
          )}
        </div>
      )}
    </section>
  );
}
