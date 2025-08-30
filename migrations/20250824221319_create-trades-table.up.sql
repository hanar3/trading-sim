-- Add up migration script here
CREATE TABLE trades (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  taker_order_id INTEGER NOT NULL,
  maker_order_id INTEGER NOT NULL,
  filled_qty INTEGER,
  FOREIGN KEY (taker_order_id) REFERENCES orders(order_id),
  FOREIGN KEY (maker_order_id) REFERENCES orders(order_id)
);
