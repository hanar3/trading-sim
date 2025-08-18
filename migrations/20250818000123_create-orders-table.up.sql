-- Add up migration script here
CREATE TABLE orders (
    order_id        INTEGER PRIMARY KEY,
    base_currency   TEXT NOT NULL,
    quote_currency  TEXT NOT NULL,
    side            INTEGER NOT NULL CHECK (side IN (1, 2)),
    quantity        INTEGER NOT NULL,
    price           INTEGER NOT NULL,
    FOREIGN KEY (base_currency) REFERENCES currencies(name),
    FOREIGN KEY (quote_currency) REFERENCES currencies(name)
);
