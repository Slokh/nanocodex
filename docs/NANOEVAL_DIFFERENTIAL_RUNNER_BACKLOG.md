# Nanoeval Differential Runner Backlog

Last reviewed: 2026-07-30

This document records the remaining Nanoeval work uncovered while comparing
Nanocodex and stock Codex on Terminal-Bench 2.1. It is the backlog for making
differential runs trustworthy, reproducible, and limited primarily by model API
capacity rather than harness overhead.

## Current baseline

The following work is already implemented and should not be reopened without
new evidence:

- Completion-driven central scheduling across tasks, arms, modes, efforts, and
  repetitions within one runner process.
- Independent slot and memory release as each arm completes, with immediate
  backfill.
- Small initial guest-memory allocations, peak-memory measurement, learned
  profiles with slack, and bounded geometric OOM retries.
- Lazy and bounded image preparation while retaining explicit out-of-band
  preparation.
- Host `AGENTS.md`, guest shell/date/timezone, Code Mode prompt, tool namespace,
  tool ordering, and tool-description parity fixes.
- Managed subprocess cleanup parity.
- Trace retention without retaining VM disks, including cancellation and setup
  cleanup paths.
- File-descriptor limit handling and bounded infrastructure replacements.
- Model-visible Responses tool-call accounting, with raw/nested events retained
  separately.
- Direct guest hostname routing for the previously identified DNS failure.

## P0: fix before another expensive full sweep

### 1. Separate generation parity from prewarm and cache parity

The validator currently compares initial transport requests too literally.
Codex can omit or fail a prewarm and replay full context for generation, while
Nanocodex can generate through `previous_response_id`. These requests may look
different on the wire while presenting equivalent effective context to the
generated turn.

Required behavior:

- Reconstruct the effective instructions, task input, model configuration, and
  tool definitions visible to the first turn that actually generates an answer.
- Use effective first-generation context to decide whether quality scores are
  comparable.
- Independently record prewarm presence, configuration, success, linkage,
  checkpoint reuse, and cache behavior for latency and token comparisons.
- Classify missing captures and failed warmups as typed operational evidence,
  rather than folding them into a generic context mismatch.
- Requeue profile-invalid or capture-invalid coordinates with a bounded retry
  policy until the requested number of valid pairs is collected. If this cannot
  be achieved, report separate quality and performance denominators explicitly.

Evidence: the latest medium gap k=20 cohort contained four Mailman pairs with
profile-validation errors and four additional incomplete comparison records.
Some Mailman generations had matching effective tools and context despite
different prewarm/link behavior.

### 2. Stabilize stock Codex startup in ephemeral homes

Each stock Codex arm receives a fresh `CODEX_HOME`. The current setup stages
authentication and cloud configuration but does not stage the model catalog
cache. Concurrent arms can therefore refresh model metadata independently.

Required behavior:

- Stage or pin the canonical Codex `models_cache.json` or equivalent catalog for
  each ephemeral home.
- Record the catalog identity/digest in run provenance.
- Preserve the pinned stock Codex model configuration and behavior.
- Avoid one catalog refresh process per arm under high concurrency.

Evidence: a retained Codex arm logged model-catalog refresh child-process
timeouts and its prewarm exposed only `exec` and `wait`, although its actual
generation later exposed the correct full Code Mode tool set.

### 3. Add scheduler-wide API-pressure admission control

Typed per-attempt retries exist, but the central scheduler does not currently
apply global backpressure to new admissions.

Required behavior:

- Feed HTTP 429, `Retry-After`, API overload, and repeated WebSocket handshake
  pressure from both arms into one scheduler signal.
- Pause only new admissions while active attempts retry or drain.
- Resume with a controlled ramp instead of immediately restoring peak
  concurrency.
- Persist pressure intervals, pause reasons, and admission changes in run
  artifacts.
- Keep model/API pressure separate from guest-memory calibration.

The latest k=20 cohort did not contain a confirmed 429, but it did contain a
30-second Nanocodex WebSocket handshake timeout and stock Codex startup refresh
timeouts under concurrency.

## P1: provenance and infrastructure robustness

### 4. Record realized benchmark-image identity

The current image manifest digest identifies the final stage's base OCI
manifest. It does not identify bytes introduced by Dockerfile `RUN` steps, such
as the MIPS task's live WAD download.

Required behavior:

- Record the base OCI manifest digest separately from a defensible realized
  rootfs/materialization identity.
- Propagate both identities through prepared-image metadata, comparison records,
  sweep summaries, and reports.
- Reject or group results with mixed realized image identities.
- Prefer canonical prebuilt image digests when the benchmark publishes them.

Nanocodex and Codex already use the same realized disk within a paired attempt,
so this does not invalidate paired-arm fairness. It is required for exact
cross-run and historical-score reproducibility.

### 5. Fail fast on whole-route `gvproxy` loss

The direct hostname-route bug is fixed. A separate historical failure mode
caused a live `gvproxy` route to disappear several minutes into a task.

Required behavior:

- Surface proxy exit and route-health signals to the evaluator immediately.
- Classify route loss as infrastructure failure and replace the coordinate
  instead of consuming the full agent timeout.
- Restart the proxy on the same endpoint only if that recovery is proven safe
  for an in-flight attempt.

No instance of this failure appeared in the latest local k=20 cohort, so it is
lower priority than the P0 parity and admission work.

### 6. Rebuild reports from clean, current-schema cohorts

Some existing charts combine pre-parity-fix runs, old nested Nano tool-call
counts, and profile-invalid records.

Required behavior:

- Reanalyze retained traces with the current model-visible tool-call schema.
- Exclude or clearly quarantine context-contaminated and profile-invalid
  cohorts.
- Report paired task success, input/output/cached tokens, latency, cache usage,
  and model-visible tool calls. Dollar estimates are not part of the primary
  analysis.
- Keep quality-valid and performance/cache-valid denominators separate.
- Include task-level k-run gaps and uncertainty, with links to exact retained
  traces for drilldown.

## P2: throughput improvements after measuring the remaining bottleneck

### 7. Coordinate admission across runner processes

One invocation schedules all of its work centrally, but independently launched
Nanoeval processes do not share host-memory or API admission state.

Choose one of the following explicit operating models:

- Submit all sweeps to one long-lived runner process; or
- Add a host-wide coordinator/lease for memory, slots, and API pressure.

Multiple uncoordinated processes must not each assume they own the full host.

### 8. Adapt to CPU, disk, network, and API pressure

Memory is measured and learned, while non-memory concurrency remains primarily
manually capped. Add telemetry-driven admission adjustments only where retained
runs show CPU, disk, network, or startup pressure preventing model-limited
throughput.

### 9. Consider task-worker VM reuse only if it is justified

The current runner uses a small VM per arm. Reusing task-worker VMs could reduce
bootstrap overhead, but increases isolation and cleanup complexity. Implement
pooling only if measurements show per-arm VM startup remains a material
bottleneck after the P0 work. Preserve fresh tenant state and retain a clean
one-attempt-per-VM fallback.

## Validation sequence

After completing the P0 work:

1. Rerun the previously divergent task set at k >= 5 and confirm that every
   requested coordinate has the intended number of quality-valid pairs.
2. Rerun the quarantined service tasks (`hf-model-inference`, `kv-store-grpc`,
   and `pypi-server`) after the managed-process cleanup parity fix.
3. Run a clean full Terminal-Bench 2.1 medium four-way matrix: stock Codex and
   Nanocodex, each in Code Mode and Code Mode Only.
4. Inspect exact requests, trajectories, verifier output, token accounting, and
   infrastructure classifications before publishing aggregate charts.
5. Expand to the remaining reasoning efforts only after the medium matrix is
   clean.

## Explicit non-fixes

Do not modify benchmark tasks or verifiers to make Nanocodex pass. In
particular, the following are not established Nanoeval defects:

- DNA primer/Tm task outcomes produced by model solution choices.
- MIPS resource or WAD strategies when both paired arms receive the same
  realized environment. Only image provenance is a Nanoeval gap.
- Genuine stochastic result flips after effective generation context is
  verified equivalent.
- Canonical benchmark verifiers that are inherently slow, including
  `filter-js-from-html`, absent evidence that Nanoeval caused the slowdown.

These cases should remain visible in task-level analysis without being
"corrected" in the benchmark environment.
