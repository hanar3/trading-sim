//! Indexed, read-only order progress. This never calls into the matching engine.
use actix_web::{HttpResponse, web};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{path::PathBuf, time::Duration};

#[derive(Clone)]
pub struct ProgressStore {
    pool: SqlitePool,
}
impl ProgressStore {
    pub fn new(file: PathBuf) -> Self {
        let options = SqliteConnectOptions::new()
            .filename(file)
            .read_only(true)
            .busy_timeout(Duration::from_millis(50));
        Self {
            pool: SqlitePoolOptions::new()
                .max_connections(2)
                .connect_lazy_with(options),
        }
    }
    async fn lookup(
        &self,
        client_id: &str,
        details: bool,
    ) -> Result<serde_json::Value, sqlx::Error> {
        // All fields and fills come from one consistent committed database view.
        let mut tx = self.pool.begin().await?;
        let order = sqlx::query("SELECT order_id, side, price, quantity, cancelled FROM orders WHERE client_order_id = ? AND client_order_id != ''")
            .bind(client_id).fetch_optional(&mut *tx).await?;
        let Some(order) = order else {
            return Ok(
                serde_json::json!({"client_order_id": client_id, "status": "awaiting_acceptance"}),
            );
        };
        let order_id: i64 = order.get("order_id");
        let quantity: i64 = order.get("quantity");
        let summary = sqlx::query("SELECT COALESCE(SUM(filled_qty), 0) AS filled, COUNT(*) AS fill_count FROM trades WHERE maker_order_id = ? OR taker_order_id = ?")
            .bind(order_id).bind(order_id).fetch_one(&mut *tx).await?;
        let filled: i64 = summary.get("filled");
        let cancelled = order.get::<i64, _>("cancelled") == 1;
        let status = if cancelled {
            "cancelled"
        } else if filled == quantity {
            "filled"
        } else if filled > 0 {
            "partially_filled"
        } else {
            "open"
        };
        let trades = if details {
            sqlx::query("SELECT id, maker_order_id, taker_order_id, price, filled_qty FROM trades WHERE maker_order_id = ? OR taker_order_id = ? ORDER BY id DESC LIMIT 100")
            .bind(order_id).bind(order_id).fetch_all(&mut *tx).await?
        } else {
            Vec::new()
        };
        let trades: Vec<_> = trades
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "trade_id": row.get::<i64, _>("id").to_string(),
                    "maker_order_id": row.get::<i64, _>("maker_order_id").to_string(),
                    "taker_order_id": row.get::<i64, _>("taker_order_id").to_string(),
                    "price": row.get::<Option<i64>, _>("price").map(|price| price.to_string()),
                    "quantity": row.get::<i64, _>("filled_qty").to_string(),
                })
            })
            .collect();
        Ok(serde_json::json!({
            "client_order_id": client_id, "order_id": order_id.to_string(), "status": status,
            "side": order.get::<i32, _>("side"), "price": order.get::<i64, _>("price").to_string(),
            "quantity": quantity.to_string(), "filled": filled.to_string(),
            "remaining": if cancelled { 0 } else { quantity - filled }.to_string(),
            "cancelled_quantity": if cancelled { quantity - filled } else { 0 }.to_string(),
            "fill_count": summary.get::<i64, _>("fill_count"), "trades": trades,
        }))
    }
}
#[derive(serde::Deserialize, Default)]
pub struct ProgressQuery {
    #[serde(default)]
    details: bool,
}

pub async fn get(
    store: web::Data<ProgressStore>,
    id: web::Path<String>,
    query: web::Query<ProgressQuery>,
) -> HttpResponse {
    if uuid::Uuid::parse_str(&id).is_err() {
        return HttpResponse::BadRequest().body("invalid client order ID");
    }
    match store.lookup(&id, query.details).await {
        Ok(value) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .json(value),
        Err(error) => {
            log::warn!("order progress unavailable: {error}");
            HttpResponse::ServiceUnavailable().body("order progress is temporarily unavailable")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> ProgressStore {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for sql in [
            include_str!("../../migrations/20250817235900_create-currencies-table.up.sql"),
            include_str!("../../migrations/20250818000123_create-orders-table.up.sql"),
            include_str!("../../migrations/20250824221319_create-trades-table.up.sql"),
            include_str!("../../migrations/20261113000100_trade_price_and_cancellation.up.sql"),
            include_str!("../../migrations/20261113000200_event_deduplication.up.sql"),
            include_str!("../../migrations/20261113000300_order_tracking.up.sql"),
        ] {
            sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        }
        ProgressStore { pool }
    }

    #[tokio::test]
    async fn tracks_open_partial_and_complete_fills_on_both_maker_and_taker_sides() {
        let store = database().await;
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            store.lookup(&id, false).await.unwrap()["status"],
            "awaiting_acceptance"
        );
        sqlx::query("INSERT INTO orders (order_id, base_currency, quote_currency, side, quantity, price, client_order_id) VALUES (1, 'BTC', 'USD', 1, 10, 100, ?), (2, 'BTC', 'USD', 2, 4, 100, ''), (3, 'BTC', 'USD', 2, 6, 100, '')")
            .bind(&id).execute(&store.pool).await.unwrap();
        assert_eq!(store.lookup(&id, false).await.unwrap()["status"], "open");
        sqlx::query("INSERT INTO trades (maker_order_id, taker_order_id, price, filled_qty) VALUES (2, 1, 100, 4)").execute(&store.pool).await.unwrap();
        let partial = store.lookup(&id, true).await.unwrap();
        assert_eq!(partial["status"], "partially_filled");
        assert_eq!(partial["filled"], "4");
        assert_eq!(partial["remaining"], "6");
        assert_eq!(partial["trades"][0]["maker_order_id"], "2");
        assert_eq!(
            store.lookup(&id, false).await.unwrap()["trades"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        sqlx::query("INSERT INTO trades (maker_order_id, taker_order_id, price, filled_qty) VALUES (1, 3, 100, 6)").execute(&store.pool).await.unwrap();
        let complete = store.lookup(&id, true).await.unwrap();
        assert_eq!(complete["status"], "filled");
        assert_eq!(complete["filled"], "10");
        assert_eq!(complete["remaining"], "0");
        assert_eq!(complete["fill_count"], 2);
    }

    #[tokio::test]
    async fn cancellation_retains_executed_quantity_and_large_values_are_exact() {
        let store = database().await;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO orders (order_id, base_currency, quote_currency, side, quantity, price, client_order_id) VALUES (1, 'BTC', 'USD', 1, 9007199254740993, 100, ?), (2, 'BTC', 'USD', 2, 3, 100, '')")
            .bind(&id).execute(&store.pool).await.unwrap();
        sqlx::query("INSERT INTO trades (maker_order_id, taker_order_id, price, filled_qty) VALUES (1, 2, 100, 3)").execute(&store.pool).await.unwrap();
        sqlx::query("UPDATE orders SET cancelled = 1 WHERE order_id = 1")
            .execute(&store.pool)
            .await
            .unwrap();
        let result = store.lookup(&id, true).await.unwrap();
        assert_eq!(result["status"], "cancelled");
        assert_eq!(result["quantity"], "9007199254740993");
        assert_eq!(result["filled"], "3");
        assert_eq!(result["remaining"], "0");
        assert_eq!(result["cancelled_quantity"], "9007199254740990");
        assert_eq!(result["trades"].as_array().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn summary_includes_fills_beyond_the_bounded_detail_list() {
        let store = database().await;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO orders (order_id, base_currency, quote_currency, side, quantity, price, client_order_id) VALUES (1, 'BTC', 'USD', 1, 101, 100, ?), (2, 'BTC', 'USD', 2, 101, 100, '')")
            .bind(&id).execute(&store.pool).await.unwrap();
        for _ in 0..101 {
            sqlx::query("INSERT INTO trades (maker_order_id, taker_order_id, price, filled_qty) VALUES (1, 2, 100, 1)").execute(&store.pool).await.unwrap();
        }
        let result = store.lookup(&id, true).await.unwrap();
        assert_eq!(result["filled"], "101");
        assert_eq!(result["status"], "filled");
        assert_eq!(result["fill_count"], 101);
        assert_eq!(result["trades"].as_array().unwrap().len(), 100);
    }
}
