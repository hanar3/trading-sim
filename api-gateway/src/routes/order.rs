use crate::messages::trading::{CancelOrder, PlaceLimitOrder, WireMessage, wire_message::Payload};
use actix_web::{HttpResponse, web};

#[derive(serde::Deserialize)]
pub struct PlaceLimitOrderJson {
    #[serde(deserialize_with = "serde_aux::field_attributes::deserialize_number_from_string")]
    pub user_id: u64,
    pub side: i32,
    #[serde(deserialize_with = "serde_aux::field_attributes::deserialize_number_from_string")]
    pub price: u64,
    #[serde(deserialize_with = "serde_aux::field_attributes::deserialize_number_from_string")]
    pub quantity: u64,
    pub base_currency: String,
    pub quote_currency: String,
}

pub async fn place_limit_order(
    form: web::Json<PlaceLimitOrderJson>,
    command_tx: web::Data<tokio::sync::mpsc::Sender<WireMessage>>,
) -> HttpResponse {
    if !matches!(form.side, 1 | 2)
        || form.price == 0
        || form.quantity == 0
        || form.price > i64::MAX as u64
        || form.quantity > i64::MAX as u64
        || form.base_currency.len() > 32
        || form.quote_currency.len() > 32
        || form.base_currency.is_empty()
        || form.quote_currency.is_empty()
    {
        return HttpResponse::BadRequest().body("invalid limit order");
    }
    let client_order_id = uuid::Uuid::new_v4().to_string();
    let wire_message = WireMessage {
        payload: Some(Payload::PlaceLimitOrder(PlaceLimitOrder {
            client_order_id: client_order_id.clone(),
            user_id: form.user_id,
            side: form.side,
            price: form.price,
            quantity: form.quantity,
            base_currency: form.base_currency.clone(),
            quote_currency: form.quote_currency.clone(),
        })),
    };

    match command_tx.try_send(wire_message) {
        Ok(_) => {
            log::info!("sent place_limit_order message to engine");
            HttpResponse::Accepted().json(serde_json::json!({"client_order_id": client_order_id}))
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            HttpResponse::ServiceUnavailable().body("command queue is full")
        }
        Err(err) => {
            log::error!(" failed to send message to engine: {err:?}");
            HttpResponse::ServiceUnavailable().finish()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CancelOrderJson {
    #[serde(deserialize_with = "serde_aux::field_attributes::deserialize_number_from_string")]
    pub order_id: u64,
}
pub async fn cancel_order(
    form: web::Json<CancelOrderJson>,
    command_tx: web::Data<tokio::sync::mpsc::Sender<WireMessage>>,
) -> HttpResponse {
    if form.order_id == 0 || form.order_id > i64::MAX as u64 {
        return HttpResponse::BadRequest().body("invalid order ID");
    }
    let wire_message = WireMessage {
        payload: Some(Payload::CancelOrder(CancelOrder {
            order_id: form.order_id,
        })),
    };

    match command_tx.try_send(wire_message) {
        Ok(_) => {
            log::info!("sent cancel order message to engine");
            HttpResponse::Accepted().finish()
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            HttpResponse::ServiceUnavailable().body("command queue is full")
        }
        Err(err) => {
            log::error!(" failed to send message to engine: {err:?}");
            HttpResponse::ServiceUnavailable().finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test};

    #[actix_web::test]
    async fn validates_inputs_and_reports_queue_acceptance_and_overload() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WireMessage>(1);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(tx))
                .route("/orders", web::post().to(place_limit_order))
                .route("/orders", web::delete().to(cancel_order)),
        )
        .await;
        for body in [
            r#"{"user_id":1,"side":0,"price":100,"quantity":5,"base_currency":"BTC","quote_currency":"USD"}"#,
            r#"{"user_id":1,"side":99,"price":100,"quantity":5,"base_currency":"BTC","quote_currency":"USD"}"#,
            r#"{"user_id":1,"side":1,"price":0,"quantity":5,"base_currency":"BTC","quote_currency":"USD"}"#,
            r#"{"user_id":1,"side":1,"price":100,"quantity":0,"base_currency":"BTC","quote_currency":"USD"}"#,
            r#"{"user_id":1,"side":1,"price":18446744073709551615,"quantity":5,"base_currency":"BTC","quote_currency":"USD"}"#,
            r#"{"user_id":1,"side":1,"price":100,"quantity":5,"base_currency":"ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFG","quote_currency":"USD"}"#,
            r#"{"user_id":1,"side":1,"price":100,"quantity":5,"base_currency":"","quote_currency":"USD"}"#,
        ] {
            let request = test::TestRequest::post()
                .uri("/orders")
                .insert_header(("content-type", "application/json"))
                .set_payload(body)
                .to_request();
            assert_eq!(
                test::call_service(&app, request).await.status(),
                StatusCode::BAD_REQUEST
            );
            assert!(rx.try_recv().is_err());
        }
        let body = r#"{"user_id":1,"side":1,"price":100,"quantity":5,"base_currency":"BTC","quote_currency":"USD"}"#;
        for status in [StatusCode::ACCEPTED, StatusCode::SERVICE_UNAVAILABLE] {
            let request = test::TestRequest::post()
                .uri("/orders")
                .insert_header(("content-type", "application/json"))
                .set_payload(body)
                .to_request();
            assert_eq!(test::call_service(&app, request).await.status(), status);
        }
        rx.recv().await.unwrap();
        drop(rx);
        let request = test::TestRequest::delete()
            .uri("/orders")
            .insert_header(("content-type", "application/json"))
            .set_payload(r#"{"order_id":1}"#)
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
