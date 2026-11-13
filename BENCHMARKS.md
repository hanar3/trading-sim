# Order-book benchmark methodology

## Reproduce

```sh
cargo bench -p engine --bench orderbook_benchmark --locked -- --save-baseline insertion-v1
```

Criterion writes HTML to `target/criterion/report/index.html` and machine-readable
estimates under `target/criterion/<scenario>/insertion-v1/`. On the same machine,
compare a subsequent implementation using `--baseline insertion-v1` in place of
`--save-baseline insertion-v1`. Run on a quiet machine and repeat before drawing
performance conclusions. RabbitMQ and the other services are not required.

The Cargo workspace pins dependency resolution through `Cargo.lock`. The bench
uses the optimized Cargo bench profile, Criterion 0.5.1, 100 samples, a three-second
warmup, and a five-second target measurement window per scenario.

Each operation starts from a fresh book with 1,000 bid levels (9999 down to 9000)
and 1,000 ask levels (10001 up to 11000), one order of quantity 10 per level.
`iter_batched_ref` with `BatchSize::PerIteration` excludes fixture construction
and destruction. Inputs pass through `black_box` and the book mutates in place.
The book is recently constructed and likely cache-hot; no cold-cache claims are made.

| Scenario | Operation |
| --- | --- |
| No match | Buy 10 at 9000, appending to an existing bid level |
| Full match | Buy 10 at 10001, removing one resting ask and its price level |
| Walk the book | Buy 50 at 10005, consuming five ask levels |
| Partial fill | Buy 5 at 10001, leaving 5 on the resting ask |
| Cancellation | Cancel resting order 1 through the ID index; queue cleanup is lazy |

## November 13, 2026 baseline

- CPU: Apple M4, 10 logical CPUs; memory: 24 GiB.
- OS: macOS 26.5 (25F71), aarch64-apple-darwin.
- Rust: rustc 1.92.0 (ded5c06cf, LLVM 21.1.3).
- Command: the baseline command above, with `--offline` after fetching dependencies.
- This development-machine run overlapped some validation/build activity. Treat it
  as a reproducible initial baseline, not a controlled hardware comparison.

| Scenario | Point estimate | Criterion 95% interval |
| --- | ---: | ---: |
| No match | 41.423 ns | 41.166–41.675 ns |
| Full match | 133.59 ns | 130.94–136.54 ns |
| Walk the book | 417.74 ns | 409.99–428.66 ns |
| Partial fill | 52.287 ns | 50.970–54.073 ns |
| Cancellation | 27.984 ns | 27.294–28.931 ns |

The checked-in [Criterion output](benchmarks/2026-11-13-insertion-v1.txt) records
sampling and outliers. These intervals describe Criterion's estimate of the
microbenchmark timing; they are not production p95/p99 latency measurements.

Earlier measurements (roughly 50–52 microseconds in the initial November run,
and 111–116 microseconds in the old README) included destruction of the entire
2,000-order book in the timed closure. The apparent difference primarily reflects
corrected measurement boundaries. **It is not evidence of an equivalent matching
speedup.**

The suite does not measure service throughput, network latency, queueing delays,
SQLite latency, broker failures, allocation counts, or production workload mixes.
Those require separate experiments. Avoid converting these timings into an
end-to-end orders-per-second claim.
