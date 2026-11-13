export interface Market {
  base_currency: string;
  quote_currency: string;
  base_scale: number;
  quote_scale: number;
}
export interface Level {
  price: string;
  quantity: string;
  order_count: number;
}
export interface OpenOrder {
  order_id: string;
  user_id: string;
  side: number;
  price: string;
  quantity: string;
  remaining: string;
}
export interface Trade {
  event_id: string;
  maker_order_id: string;
  taker_order_id: string;
  side: number;
  price: string;
  quantity: string;
}
export interface Snapshot {
  status: "live" | "syncing" | "unavailable";
  sequence: string;
  updated_at: number;
  market: Market;
  bids: Level[];
  asks: Level[];
  open_orders: OpenOrder[];
  open_order_count: number;
  trades: Trade[];
}
export const I64_MAX = 9223372036854775807n;
// Keep integers exact across HTTP and JavaScript, including values beyond 2^53.
export function parseUnits(value: string, scale: number): string {
  const input = value.trim();
  if (!/^\d+(\.\d+)?$/.test(input))
    throw new Error("Enter a positive decimal number.");
  const [whole, fraction = ""] = input.split(".");
  if (fraction.length > scale)
    throw new Error(`Use at most ${scale} decimal places.`);
  const units =
    BigInt(whole) * 10n ** BigInt(scale) +
    BigInt(fraction.padEnd(scale, "0") || "0");
  if (units <= 0n || units > I64_MAX)
    throw new Error("Value is outside the supported range.");
  return units.toString();
}
export function participantId(value: string): string {
  if (!/^\d+$/.test(value) || BigInt(value) < 1n || BigInt(value) > I64_MAX)
    throw new Error("Participant ID must be a positive signed 64-bit integer.");
  return BigInt(value).toString();
}
export function formatUnits(
  value: string,
  scale: number,
  group = true,
): string {
  const units = BigInt(value),
    negative = units < 0n;
  const raw = (negative ? -units : units).toString().padStart(scale + 1, "0");
  const whole = scale === 0 ? raw : raw.slice(0, -scale);
  const fraction = scale === 0 ? "" : `.${raw.slice(-scale)}`;
  return `${negative ? "−" : ""}${group ? whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : whole}${fraction}`;
}
export function depthWidth(quantity: string, max: bigint): number {
  return max === 0n ? 0 : Number((BigInt(quantity) * 10000n) / max) / 100;
}
export async function getSnapshot(signal?: AbortSignal): Promise<Snapshot> {
  const response = await fetch("/api/market", { signal });
  if (!response.ok)
    throw new Error(
      "The market feed is unavailable. Check that the services are running.",
    );
  return response.json();
}
export async function submitOrder(body: OrderCommand): Promise<OrderReceipt> {
  const response = await command("POST", body);
  return response.json();
}
export async function cancelOrder(orderId: string): Promise<void> {
  await command("DELETE", { order_id: orderId });
}
async function command(method: string, body: object): Promise<Response> {
  const response = await fetch("/api/orders", {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (response.status !== 202) {
    const detail = await response.text();
    throw new CommandError(
      response.status === 503
        ? "The command queue is unavailable. Try again shortly."
        : detail || "The command could not be queued.",
      response.status === 400 || response.status === 503,
    );
  }
  return response;
}

export interface OrderCommand {
  user_id: string;
  side: number;
  price: string;
  quantity: string;
  base_currency: string;
  quote_currency: string;
}
export interface OrderReceipt {
  client_order_id: string;
}
export interface OrderFill {
  trade_id: string;
  maker_order_id: string;
  taker_order_id: string;
  price: string | null;
  quantity: string;
}
export interface OrderProgress {
  client_order_id: string;
  status:
    | "awaiting_acceptance"
    | "open"
    | "partially_filled"
    | "filled"
    | "cancelled";
  order_id?: string;
  side?: number;
  price?: string;
  quantity?: string;
  filled?: string;
  remaining?: string;
  cancelled_quantity?: string;
  fill_count?: number;
  trades?: OrderFill[];
}
export class CommandError extends Error {
  constructor(
    message: string,
    public readonly definite: boolean,
  ) {
    super(message);
  }
}
export async function getOrderProgress(
  clientId: string,
  details: boolean,
  signal?: AbortSignal,
): Promise<OrderProgress> {
  const response = await fetch(
    `/api/orders/status/${encodeURIComponent(clientId)}?details=${details}`,
    { signal },
  );
  if (!response.ok)
    throw new Error("Progress updates are temporarily unavailable.");
  return response.json();
}
export function fillPercent(filled: string, quantity: string): number {
  const total = BigInt(quantity);
  return total > 0n ? Number((BigInt(filled) * 10000n) / total) / 100 : 0;
}
export function isTerminal(progress?: OrderProgress): boolean {
  return progress?.status === "filled" || progress?.status === "cancelled";
}
