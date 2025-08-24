-- Add up migration script here
CREATE TABLE instruments (
	name TEXT UNIQUE PRIMARY KEY,
	scaling_factor INTEGER NOT NULL
);

INSERT INTO instruments (name, scaling_factor) VALUES ('USD', 2);
INSERT INTO instruments (name, scaling_factor) VALUES ('BTC', 8);
INSERT INTO instruments (name, scaling_factor) VALUES ('ETH', 8);

