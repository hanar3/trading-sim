//! Browser market data is projected from the committed event log, never queried
//! from the matching thread. One indexed reader and one latest-value watch channel
//! serve every browser, independently of client count and rendering speed.
use crate::messages::trading::{WireMessage, wire_message::Payload};
use actix_web::{HttpResponse, web};
use prost::Message;
use serde::Serialize;
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

const LEVELS: usize = 20;
const OPEN_ORDERS: usize = 100;
const RECENT_TRADES: usize = 40;
const BATCH: i64 = 2048;
const MAX_BATCHES: usize = 8;

#[derive(Clone, Debug, serde::Deserialize, Serialize)]
#[serde(default)]
pub struct MarketSettings {
    pub base_currency: String,
    pub quote_currency: String,
    pub base_scale: u8,
    pub quote_scale: u8,
}
impl Default for MarketSettings {
    fn default() -> Self {
        Self {
            base_currency: "BTC".into(),
            quote_currency: "USD".into(),
            base_scale: 8,
            quote_scale: 2,
        }
    }
}

#[derive(Clone, serde::Deserialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub file: PathBuf,
}
impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            file: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../message-persistor/db.sqlite"),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Level {
    price: String,
    quantity: String,
    order_count: usize,
}
#[derive(Clone, Serialize)]
pub struct OpenOrder {
    order_id: String,
    user_id: String,
    side: i32,
    price: String,
    quantity: String,
    remaining: String,
}
#[derive(Clone, Serialize)]
pub struct Trade {
    event_id: String,
    maker_order_id: String,
    taker_order_id: String,
    side: i32,
    price: String,
    quantity: String,
}
#[derive(Serialize)]
struct Snapshot {
    status: &'static str,
    sequence: String,
    updated_at: u64,
    market: MarketSettings,
    bids: Vec<Level>,
    asks: Vec<Level>,
    open_orders: Vec<OpenOrder>,
    open_order_count: usize,
    trades: Vec<Trade>,
}
struct Order {
    id: u64,
    user: u64,
    side: i32,
    price: u64,
    quantity: u64,
    remaining: u64,
}
#[derive(Default)]
struct PriceLevel {
    quantity: u128,
    count: usize,
}
#[derive(Default)]
struct Projection {
    cursor: i64,
    updated_at: u64,
    orders: HashMap<u64, Order>,
    // Separate ordered indexes keep publication proportional to displayed depth.
    bids: BTreeMap<u64, PriceLevel>,
    asks: BTreeMap<u64, PriceLevel>,
    order_ids: BTreeMap<u64, ()>,
    trades: VecDeque<Trade>,
}
impl Projection {
    fn reduce(&mut self, id: u64, amount: Option<u64>) -> Result<(), &'static str> {
        let order = self
            .orders
            .get_mut(&id)
            .ok_or("event references unknown order")?;
        let quantity = amount.unwrap_or(order.remaining);
        if quantity > order.remaining {
            return Err("fill exceeds remaining quantity");
        }
        order.remaining -= quantity;
        let levels = if order.side == 1 {
            &mut self.bids
        } else {
            &mut self.asks
        };
        let level = levels.get_mut(&order.price).ok_or("missing price level")?;
        level.quantity -= u128::from(quantity);
        if order.remaining == 0 {
            level.count -= 1;
            if level.count == 0 {
                levels.remove(&order.price);
            }
            self.orders.remove(&id);
            self.order_ids.remove(&id);
        }
        Ok(())
    }
    fn apply(&mut self, id: &str, bytes: &[u8]) -> Result<(), &'static str> {
        let message = WireMessage::decode(bytes).map_err(|_| "invalid committed event")?;
        match message.payload {
            Some(Payload::OrderAccepted(order)) => {
                if !matches!(order.side, 1 | 2)
                    || order.quantity == 0
                    || self.orders.contains_key(&order.order_id)
                {
                    return Err("invalid accepted order");
                }
                let levels = if order.side == 1 {
                    &mut self.bids
                } else {
                    &mut self.asks
                };
                let level = levels.entry(order.price).or_default();
                level.quantity += u128::from(order.quantity);
                level.count += 1;
                self.order_ids.insert(order.order_id, ());
                self.orders.insert(
                    order.order_id,
                    Order {
                        id: order.order_id,
                        user: order.user_id,
                        side: order.side,
                        price: order.price,
                        quantity: order.quantity,
                        remaining: order.quantity,
                    },
                );
            }
            Some(Payload::TradeOccurred(trade)) => {
                let taker = self
                    .orders
                    .get(&trade.taker_order_id)
                    .ok_or("missing taker")?;
                let maker = self
                    .orders
                    .get(&trade.maker_order_id)
                    .ok_or("missing maker")?;
                if trade.quantity == 0
                    || trade.taker_order_id == trade.maker_order_id
                    || taker.side == maker.side
                    || trade.quantity > taker.remaining
                    || trade.quantity > maker.remaining
                {
                    return Err("invalid committed trade");
                }
                let side = taker.side;
                self.reduce(trade.maker_order_id, Some(trade.quantity))?;
                self.reduce(trade.taker_order_id, Some(trade.quantity))?;
                self.trades.push_front(Trade {
                    event_id: id.into(),
                    maker_order_id: trade.maker_order_id.to_string(),
                    taker_order_id: trade.taker_order_id.to_string(),
                    side,
                    price: trade.price.to_string(),
                    quantity: trade.quantity.to_string(),
                });
                self.trades.truncate(RECENT_TRADES);
            }
            Some(Payload::OrderCancelled(order)) => self.reduce(order.order_id, None)?,
            _ => return Err("unexpected committed event"),
        }
        Ok(())
    }
    fn snapshot(&self, market: &MarketSettings, status: &'static str) -> Snapshot {
        let level = |(price, level): (&u64, &PriceLevel)| Level {
            price: price.to_string(),
            quantity: level.quantity.to_string(),
            order_count: level.count,
        };
        Snapshot {
            status,
            sequence: self.cursor.to_string(),
            updated_at: self.updated_at,
            market: market.clone(),
            bids: self.bids.iter().rev().take(LEVELS).map(level).collect(),
            asks: self.asks.iter().take(LEVELS).map(level).collect(),
            open_orders: self
                .order_ids
                .keys()
                .rev()
                .take(OPEN_ORDERS)
                .map(|id| {
                    let order = &self.orders[id];
                    OpenOrder {
                        order_id: order.id.to_string(),
                        user_id: order.user.to_string(),
                        side: order.side,
                        price: order.price.to_string(),
                        quantity: order.quantity.to_string(),
                        remaining: order.remaining.to_string(),
                    }
                })
                .collect(),
            open_order_count: self.orders.len(),
            trades: self.trades.iter().cloned().collect(),
        }
    }
}

struct Publication {
    json: web::Bytes,
    sse: web::Bytes,
}
#[derive(Clone)]
pub struct MarketFeed {
    latest: watch::Sender<Arc<Publication>>,
}
impl MarketFeed {
    pub fn new(market: &MarketSettings) -> Self {
        let publication = publish(&Projection::default(), market, "syncing");
        Self {
            latest: watch::channel(Arc::new(publication)).0,
        }
    }
    /// A single reader for all clients; no request ever performs a database query.
    pub async fn run(self, file: PathBuf, market: MarketSettings) {
        let options = SqliteConnectOptions::new()
            .filename(file)
            .read_only(true)
            .busy_timeout(Duration::from_millis(50));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy_with(options);
        let mut projection = Projection::default();
        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut status = "syncing";
        loop {
            ticker.tick().await;
            let before = projection.cursor;
            let result = async {
                for _ in 0..MAX_BATCHES {
                    let rows = sqlx::query("SELECT rowid, event_id, payload FROM processed_events WHERE rowid > ? ORDER BY rowid LIMIT ?")
                        .bind(projection.cursor).bind(BATCH).fetch_all(&pool).await.map_err(|e| e.to_string())?;
                    let drained = rows.len() < BATCH as usize;
                    for row in rows {
                        let bytes: Vec<u8> = row.get("payload");
                        projection.apply(row.get("event_id"), &bytes).map_err(str::to_string)?;
                        projection.cursor = row.get("rowid");
                    }
                    if drained { return Ok::<_, String>("live"); }
                    tokio::task::yield_now().await;
                }
                Ok("syncing")
            }.await;
            let next_status = match result {
                Ok(value) => value,
                Err(error) => {
                    if status != "unavailable" {
                        log::warn!("market projection unavailable: {error}");
                    }
                    "unavailable"
                }
            };
            if before != projection.cursor {
                projection.updated_at = now();
            }
            if before != projection.cursor || next_status != status {
                self.latest
                    .send_replace(Arc::new(publish(&projection, &market, next_status)));
            }
            status = next_status;
        }
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn publish(projection: &Projection, market: &MarketSettings, status: &'static str) -> Publication {
    let json = serde_json::to_string(&projection.snapshot(market, status))
        .expect("market snapshot is serializable");
    Publication {
        sse: web::Bytes::from(format!("event: book\ndata: {json}\n\n")),
        json: web::Bytes::from(json),
    }
}

pub async fn snapshot(feed: web::Data<MarketFeed>) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .content_type("application/json")
        .body(feed.latest.borrow().json.clone())
}
pub async fn stream(feed: web::Data<MarketFeed>) -> HttpResponse {
    let receiver = feed.latest.subscribe();
    let stream =
        futures_util::stream::unfold((receiver, true), |(mut receiver, first)| async move {
            let bytes = if first {
                receiver.borrow_and_update().sse.clone()
            } else {
                match tokio::time::timeout(Duration::from_secs(10), receiver.changed()).await {
                    Ok(Ok(())) => receiver.borrow_and_update().sse.clone(),
                    Ok(Err(_)) => return None,
                    Err(_) => web::Bytes::from_static(b": keepalive\n\n"),
                }
            };
            Some((Ok::<_, actix_web::Error>(bytes), (receiver, false)))
        });
    HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache, no-transform"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::trading::{OrderAccepted, OrderCancelled, TradeOccurred};

    fn apply(model: &mut Projection, payload: Payload) {
        model
            .apply(
                "test-event",
                &WireMessage {
                    payload: Some(payload),
                }
                .encode_to_vec(),
            )
            .unwrap();
    }
    fn order(id: u64, side: i32, price: u64, quantity: u64) -> Payload {
        Payload::OrderAccepted(OrderAccepted {
            client_order_id: String::new(),
            order_id: id,
            user_id: 1,
            side,
            price,
            quantity,
            base_currency: "BTC".into(),
            quote_currency: "USD".into(),
        })
    }
    #[test]
    fn partial_fills_and_cancellations_keep_aggregated_depth_consistent() {
        let mut model = Projection::default();
        apply(&mut model, order(1, 2, 100, 10));
        apply(&mut model, order(2, 2, 100, 7));
        apply(&mut model, order(3, 1, 100, 4));
        apply(
            &mut model,
            Payload::TradeOccurred(TradeOccurred {
                maker_order_id: 1,
                taker_order_id: 3,
                price: 100,
                quantity: 4,
            }),
        );
        let snapshot = model.snapshot(&MarketSettings::default(), "live");
        assert_eq!(snapshot.asks[0].quantity, "13");
        assert_eq!(snapshot.asks[0].order_count, 2);
        assert!(snapshot.bids.is_empty());
        assert_eq!(snapshot.open_order_count, 2);
        assert_eq!(snapshot.trades[0].side, 1);
        assert_eq!(snapshot.open_orders[1].remaining, "6");
        apply(
            &mut model,
            Payload::OrderCancelled(OrderCancelled { order_id: 1 }),
        );
        assert_eq!(model.asks[&100].quantity, 7);
        apply(&mut model, order(4, 1, 100, 7));
        apply(
            &mut model,
            Payload::TradeOccurred(TradeOccurred {
                maker_order_id: 2,
                taker_order_id: 4,
                price: 100,
                quantity: 7,
            }),
        );
        assert!(model.orders.is_empty());
        assert!(model.bids.is_empty());
        assert!(model.asks.is_empty());
    }
    #[test]
    fn bounded_snapshots_are_sorted_and_keep_large_aggregate_quantities_exact() {
        let mut model = Projection::default();
        for id in 1..=120 {
            apply(&mut model, order(id, 1, id, i64::MAX as u64));
        }
        for id in 121..=123 {
            apply(&mut model, order(id, 2, 200, i64::MAX as u64));
        }
        let snapshot = model.snapshot(&MarketSettings::default(), "live");
        assert_eq!(snapshot.bids.len(), LEVELS);
        assert_eq!(snapshot.bids[0].price, "120");
        assert_eq!(snapshot.open_orders.len(), OPEN_ORDERS);
        assert_eq!(snapshot.open_orders[0].order_id, "123");
        assert_eq!(
            snapshot.asks[0].quantity,
            (u128::from(i64::MAX as u64) * 3).to_string()
        );
        let encoded = serde_json::to_value(&snapshot).unwrap();
        assert!(encoded["asks"][0]["quantity"].is_string());
    }
    #[test]
    fn malformed_or_inconsistent_events_fail_instead_of_inventing_a_book() {
        let mut model = Projection::default();
        assert!(model.apply("bad", &[255]).is_err());
        apply(&mut model, order(1, 1, 100, 5));
        assert!(
            model
                .apply(
                    "bad",
                    &WireMessage {
                        payload: Some(Payload::TradeOccurred(TradeOccurred {
                            maker_order_id: 9,
                            taker_order_id: 1,
                            price: 100,
                            quantity: 5
                        }))
                    }
                    .encode_to_vec()
                )
                .is_err()
        );
        assert_eq!(model.bids[&100].quantity, 5);
    }
    #[tokio::test]
    async fn a_slow_subscriber_only_retains_the_latest_snapshot() {
        let market = MarketSettings::default();
        let feed = MarketFeed::new(&market);
        let mut subscriber = feed.latest.subscribe();
        for cursor in 1..=1000 {
            let projection = Projection {
                cursor,
                ..Projection::default()
            };
            feed.latest
                .send_replace(Arc::new(publish(&projection, &market, "live")));
        }
        subscriber.changed().await.unwrap();
        let publication = subscriber.borrow_and_update();
        let value: serde_json::Value = serde_json::from_slice(&publication.json).unwrap();
        assert_eq!(value["sequence"], "1000");
    }
}
