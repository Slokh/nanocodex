# Terminal-Bench 2.1 differential evidence

Last reviewed: 2026-07-30

This is the source-controlled audit summary for Nanocodex versus released
stock Codex. Complete captures, events, trajectories, verifier output,
workspaces, and run records remain in evaluator-owned storage outside Git.

## Protocol

- Dataset: `terminal-bench/terminal-bench-2-1`, revision
  `sha256:7d7bdc1cbedad549fc1140404bd4dc45e5fd0ea7c4186773687d177ad3a0699a`
  (89 canonical tasks).
- Profile: `gpt-5.6-sol`, medium effort, web search and multi-agent disabled.
- Comparator: released `codex-cli 0.145.0`; each run retains its executable
  digest.
- Isolation: one disposable microVM per arm, identical task package and
  canonical verifier, independent model samples.
- Decision-grade cells require five trials per treatment. A trial is admitted
  only with complete profile validation, API capture, trajectory, verifier
  output, and terminal outcome.
- Score, lifecycle failure, cleanup failure, and billing completeness remain
  independent axes. Tasks and verifiers are never modified.

Authoritative run directories contain `comparison.json`,
`api-comparison.json`, `progress.jsonl`, both arm records, trajectories, and
verifier artifacts. Derived comparisons can be regenerated without rerunning
agents or VMs.

## Runner and scheduler evidence

The campaign host had 32 logical CPUs and 62 GiB RAM. Admission bounded both
slots and declared guest memory at 48 GiB, released paired arms independently,
and backfilled later runnable work. Shared image/runtime caches did not share
workspaces, process trees, network state, or verifiers.

Focused checks cover ordinary and paired VM execution, auxiliary-request
filtering, quiet-work heartbeats, response and tool-result lineage, offline
reanalysis, bounded infrastructure replacement, learned memory with geometric
OOM retry, subprocess cleanup, cancellation, and resumable finite jobs. A
generation counter closes the capacity-probe/notification lost-wake window;
competing invocations wait for capacity instead of reporting a false
unschedulable queue.

## Historical evidence

The initial k=1 Code-Mode-Only inventory scored 79/89 for Nanocodex and 67/89
for stock, but predates later context and lifecycle corrections and is not a
release comparison.

A later controlled audit completed both stock-tool-mode cells for 81 tasks:

| Stock treatment | Pairs | Both | Nano only | Stock only | Neither | Nano | Stock |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| normal Code Mode | 405 | 307 | 29 | 26 | 43 | 336 | 333 |
| Code-Mode-Only | 405 | 300 | 34 | 31 | 40 | 334 | 331 |

The exact sign-test p-values were 0.7877 and 0.8043; pooled discordant wins
were 63 versus 57 (`p=0.6483`). These exploratory cells had no preregistered
stopping rule. Eight incomplete tasks remained outside the denominator.

All 810 included pairs were scored and comparable, retained stable cache
identity, and had no broken response or tool-result links. Initial
model-visible text matched in 809/810 pairs; Code-Mode-Only nested definitions
matched in 405/405. Forty-five pairs retried or replayed history, with symmetric
exclusive wins. No retained evidence identified a response-chain, replay,
cache, or tool-result defect.

## Corrected exclusions

Three service tasks exposed a VM-adapter bug: Nanocodex foreground sessions
survived into verification while stock-owned processes did not. Managed process
groups now terminate before verification for both arms; deliberately detached
services may survive. Those old cells remain regression evidence and are
excluded until rerun.

Later corrections also removed irrelevant stock capabilities, matched the Code
Mode catalog, derived shell and timezone from each guest, and constrained
`AGENTS.md` discovery to the ready guest filesystem. Comparison schema v15
rejects prompt, tool, grammar, ordering, shell, timezone, or instruction drift.
Historical aggregates therefore remain engineering evidence, not a corrected
release result.

## Release gate

Publish only a fresh commit-pinned schema-v15 cohort with five valid pairs per
selected cell, no infrastructure replacements in the score denominator, and
manual inspection of every discordant verifier artifact. Measure throughput
and low-contention latency separately.
