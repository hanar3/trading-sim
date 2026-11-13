// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { OrderTracker, type TrackedOrder } from "./order-tracker";
import { getOrderProgress, type OrderProgress } from "@/lib/market";

vi.mock("@/lib/market", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/market")>()),
  getOrderProgress: vi.fn(),
}));
const tracked: TrackedOrder = {
  id: "local-1",
  clientId: "server-1",
  phase: "queued",
  submittedAt: 1000,
  market: {
    base_currency: "BTC",
    quote_currency: "USD",
    base_scale: 0,
    quote_scale: 0,
  },
  order: {
    user_id: "1",
    side: 1,
    price: "100",
    quantity: "10",
    base_currency: "BTC",
    quote_currency: "USD",
  },
};
const partial: OrderProgress = {
  client_order_id: "server-1",
  order_id: "1",
  status: "partially_filled",
  quantity: "10",
  filled: "4",
  remaining: "6",
  cancelled_quantity: "0",
  fill_count: 1,
  trades: [
    {
      trade_id: "42",
      maker_order_id: "1",
      taker_order_id: "2",
      price: "100",
      quantity: "4",
    },
  ],
};
function mount(order = tracked) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, retryDelay: 0, gcTime: 0 } },
  });
  const dismiss = vi.fn();
  const view = render(
    <QueryClientProvider client={client}>
      <OrderTracker tracked={order} dismiss={dismiss} />
    </QueryClientProvider>,
  );
  return { client, dismiss, ...view };
}
beforeEach(() => {
  vi.mocked(getOrderProgress).mockReset();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("persistent order activity", () => {
  it("stays queued beyond the old toast timeout and only closes on request", async () => {
    vi.useFakeTimers();
    const { dismiss } = mount({ ...tracked, clientId: undefined });
    await act(async () => {
      vi.advanceTimersByTime(15000);
    });
    expect(screen.getByRole("status").textContent).toBe("Queued");
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      "0",
    );
    expect(dismiss).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Dismiss.*tracker/ }));
    expect(dismiss).toHaveBeenCalledWith("local-1");
  });
  it("shows partial fills and fetches matched trades only after expansion", async () => {
    vi.mocked(getOrderProgress).mockImplementation(async (_id, details) => ({
      ...partial,
      trades: details ? partial.trades : [],
    }));
    mount();
    await screen.findByText("Partially filled");
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      "40",
    );
    expect(screen.queryByText("Trade #42")).toBeNull();
    expect(
      vi.mocked(getOrderProgress).mock.calls.every((call) => call[1] === false),
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Show details" }));
    await screen.findByText("Trade #42");
    expect(screen.getByText("Maker · Matched #2")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: "Hide details" })
        .getAttribute("aria-expanded"),
    ).toBe("true");
  });
  it("updates progress to complete without dismissing the tracker", async () => {
    vi.mocked(getOrderProgress).mockResolvedValue(partial);
    const { client, dismiss } = mount();
    await screen.findByText("Partially filled");
    vi.mocked(getOrderProgress).mockResolvedValue({
      ...partial,
      status: "filled",
      filled: "10",
      remaining: "0",
      fill_count: 2,
    });
    await act(async () => {
      await client.invalidateQueries({ queryKey: ["order-progress"] });
    });
    await waitFor(() =>
      expect(
        screen.getByRole("progressbar").getAttribute("aria-valuenow"),
      ).toBe("100"),
    );
    expect(screen.getByRole("status").textContent).toBe("Filled");
    expect(dismiss).not.toHaveBeenCalled();
  });
  it("keeps the executed fraction when remaining quantity is cancelled", async () => {
    vi.mocked(getOrderProgress).mockResolvedValue({
      ...partial,
      status: "cancelled",
      filled: "3",
      remaining: "0",
      cancelled_quantity: "7",
    });
    mount();
    await screen.findByText("Cancelled");
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      "30",
    );
    expect(screen.getByText(/7 BTC cancelled/)).toBeTruthy();
  });
  it("retains confirmed progress when expanded trade details are unavailable", async () => {
    vi.mocked(getOrderProgress).mockResolvedValue({ ...partial, trades: [] });
    mount();
    await screen.findByText("Partially filled");
    vi.mocked(getOrderProgress).mockRejectedValue(new Error("offline"));
    fireEvent.click(screen.getByRole("button", { name: "Show details" }));
    await screen.findByText(/Updates paused/);
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
      "40",
    );
    expect(screen.getByRole("status").textContent).toBe("Partially filled");
    expect(
      screen.getByText("Matched trade details are temporarily unavailable."),
    ).toBeTruthy();
  });
});
