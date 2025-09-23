## Trading-sim

A naive trading simulator implementation. Implements a somewhat performant (microseconds) order-book that as of the writing of this, supports market and limit trading.

The main purpose of this project is to provide an API for those who just want to play around and understand trading. As a user, one has full power over both sides of the order
book, and is not restricted by any balances.

## Modules
- **api-gateway**: this process is responsible for taking orders in as JSON, converting it to Protobuf and sending over TCP to the `engine` module.
  - TCP connection is implemented with exponential backoff to handle eventual reconnections
  - protobuf is used to optimize reads on engine side, we want this to be as fast as possible (could use FIX?)
  - tests: benchmarks are done with [**critetion**](https://github.com/bheisler/criterion.rs), `cargo bench` to run the benchmarks.
- **engine**: accepts commands over TCP, orchestrates order book, sends events over to a high-thorughput queue (rabbitMQ)
- **message-persistor**: reads events from queue, persists them onto a sqlite database.
- **migrations**: database migrations
- **proto**: describes protobuf messages.

## Future/Roadmap
- [ ] Frontend
  -  [ ] Placing orders
  - [ ] Candle charts
  
- [ ] Backend 
  - [ ] Place market order
  - [x] Place limit order
  - [x] Cancel limit order
  - [x] Modify limit order
  - [x] Benchkmarks
  - [ ] Serve candles
- [ ] Bots to spam orders and grow the order book?
- [ ] Docker to spin up multiple engine instances + service discovery to talk to them?

## Last benchmark (Sept, 22nd 2025)

```bash
add_limit_order_no_match
                        time:   [111.11 µs 111.37 µs 111.69 µs]

add_limit_order_full_match_one
                        time:   [111.18 µs 111.31 µs 111.48 µs]

Benchmarking add_limit_order_walk_the_book: Collecting 100 samples in estimated 5.6571 s (1
add_limit_order_walk_the_book
                        time:   [115.76 µs 116.27 µs 116.73 µs]
```
