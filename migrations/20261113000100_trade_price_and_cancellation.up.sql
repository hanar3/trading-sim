-- Historical trade prices are unknown; preserve them as NULL rather than inventing a price.
ALTER TABLE trades ADD COLUMN price INTEGER CHECK (price > 0);
ALTER TABLE orders ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0 CHECK (cancelled IN (0, 1));
