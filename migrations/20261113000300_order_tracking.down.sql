DROP INDEX trades_taker_order_id;
DROP INDEX trades_maker_order_id;
DROP INDEX orders_client_order_id;
ALTER TABLE orders DROP COLUMN client_order_id;
