# Evaluation control-plane baseline — 2026-07-26

Compact retained baseline for evaluator consolidation. Raw Criterion output and
live jobs remain outside Git. The original sample used an Apple M1 Max with 32
GiB, macOS 26.3.1, Rust 1.97.1, Criterion 0.7, 30 optimized samples, and warm
filesystem/job metadata; a focused rerun used x86_64 Linux.

Run with `just bench-eval`. Package-size gates select a retained task through
`NANOCODEX_EVAL_BENCH_TASK`.

| Operation | Apple estimate | x86_64 Linux estimate |
| --- | ---: | ---: |
| load checked-in task | 39.826–40.463 µs | not rerun |
| plan 3 tasks × 4 agents × 5 trials | 735.76–737.19 ns | 720.34–721.68 ns |
| reopen incomplete job | 145.64–147.51 µs | 26.553–26.662 µs |

Machine-local investigation budgets are 100 µs for warm task load, 10 µs for
planning 60 attempts, and 500 µs for reopening a job. These are trend signals,
not portable CI thresholds. Representative traces and packages are required
for performance claims; synthetic projection/aggregate fixtures were removed.

## Structural contract

- Expansion is deterministic and performs no network/environment work.
- Slot and memory admission are atomic and work-conserving.
- Finite identity is durable before execution; completed work resumes once.
- Each accepted attempt emits one terminal event and atomically publishes its
  result; optional events and Harbor never gate typed results.
- Partial failures commit no tool calls or history.
- Image preparation stays outside warm execution.
- Cancellation reaps descendants and releases admission.
- Score, lifecycle, cleanup, usage, and cost axes remain independent; absent
  pricing or usage is absent, never zero.

Live claims require a fresh VM-first run from the reviewed commit retaining
build, task, image, scheduler, event, ATIF, verifier, workspace, and cleanup
evidence. See `docs/TERMINAL_BENCH_2_1_DIFF_LOG.md` for differential evidence.
