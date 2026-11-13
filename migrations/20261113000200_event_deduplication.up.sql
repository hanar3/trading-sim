-- Insert this receipt and apply the event in one transaction before acknowledging AMQP.
CREATE TABLE processed_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    payload BLOB NOT NULL
);
