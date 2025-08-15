use std::sync::mpsc::Sender;

use crate::messages::trading::{PlaceLimitOrder, WireMessage, wire_message::Payload};
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
    command_tx: web::Data<Sender<WireMessage>>,
) -> HttpResponse {
    let wire_message = WireMessage {
        payload: Some(Payload::PlaceLimitOrder(PlaceLimitOrder {
            user_id: form.user_id,
            side: form.side,
            price: form.price,
            quantity: form.quantity,
        })),
    };

    match command_tx.send(wire_message) {
        Ok(_) => {
            log::info!("sent wire message to engine: {wire_message:?}");
            return HttpResponse::Ok().finish();
        }
        Err(err) => {
            log::error!(" failed to send message to engine: {err:?}");
            return HttpResponse::InternalServerError().finish();
        }
    }
}
