ALTER TABLE orders ADD COLUMN client_order_id TEXT NOT NULL DEFAULT '';
CREATE UNIQUE INDEX orders_client_order_id ON orders(client_order_id) WHERE client_order_id != '';
CREATE INDEX trades_maker_order_id ON trades(maker_order_id);
CREATE INDEX trades_taker_order_id ON trades(taker_order_id);
