-- Add up migration script here
CREATE TABLE orders (
	order_id INTEGER PRIMARY KEY,
	symbol TEXT,
	side INTEGER NOT NULL CHECK(side IN (1, 2)),
	quantity INTEGER NOT NULL,
	price INTEGER NOT NULL
);
