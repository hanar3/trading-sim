use std::sync::mpsc::Sender;

use crate::messages::trading::{CancelOrder, PlaceLimitOrder, WireMessage, wire_message::Payload};
use actix_web::{HttpResponse, web};

#[derive(serde::Deserialize)]
pub struct PlaceLimitOrderJson {
    pub user_id: u64,
    pub side: i32,
    pub price: u64,
    pub quantity: u64,
}

pub async fn place_limit_order(
    form: web::Json<PlaceLimitOrderJson>,
    command_tx: web::Data<tokio::sync::mpsc::Sender<WireMessage>>,
) -> HttpResponse {
    let wire_message = WireMessage {
        payload: Some(Payload::PlaceLimitOrder(PlaceLimitOrder {
            user_id: form.user_id,
            side: form.side,
            price: form.price,
            quantity: form.quantity,
        })),
    };

    match command_tx.send(wire_message).await {
        Ok(_) => {
            log::info!("sent place_limit_order message to engine: {wire_message:?}");
            return HttpResponse::Ok().finish();
        }
        Err(err) => {
            log::error!(" failed to send message to engine: {err:?}");
            return HttpResponse::InternalServerError().finish();
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CancelOrderJson {
    pub order_id: u64,
}
pub async fn cancel_order(
    form: web::Json<CancelOrderJson>,
    command_tx: web::Data<tokio::sync::mpsc::Sender<WireMessage>>,
) -> HttpResponse {
    let wire_message = WireMessage {
        payload: Some(Payload::CancelOrder(CancelOrder {
            order_id: form.order_id,
        })),
    };

    match command_tx.send(wire_message).await {
        Ok(_) => {
            log::info!("sent cancel order message to engine: {wire_message:?}");
            return HttpResponse::Ok().finish();
        }
        Err(err) => {
            log::error!(" failed to send message to engine: {err:?}");
            return HttpResponse::InternalServerError().finish();
        }
    }
}
