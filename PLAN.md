# Nanocodex plan

## Objective

Build a lean, library-first reimplementation of the supported Codex agent loop:
meet or beat pinned stock-Codex benchmark performance and robustness with
Pi-like customizability. Nanocodex owns the complete agent lifecycle and typed
OpenAI boundary without inheriting an app server, generic provider layer,
approval framework, or other product surface that an embedding application
does not need.

The development method is differential and eval-driven. We run Nanocodex and a
pinned stock-Codex binary on the same tasks with the same model, effort, tool
mode, and task inputs; compare their behavior while it is happening; and turn
demonstrated differences in the event loop, context construction, cache use, or
tools into focused changes and regression tests.

Every stable crate must remain useful independently, documented from its own
README, tested through public paths, benchmarked at the boundaries it can
affect, and observable without adopting the Nanocodex CLI.

## Delivery stack

### Part 1 — Stable agent and API foundations

PR #50 established the library boundaries and the first reviewed Codex-parity
checkpoint:

- `nanocodex-oai-api` owns the typed Responses boundary, retained context,
  retry/reconnect policy, telemetry, and generic Tower client.
- `nanocodex-tools` owns Code Mode, built-in tools, MCP, deferred tool search,
  and remote dispatch.
- `nanocodex-agent` owns the private driver, lifecycle, branching, snapshots,
  and rollouts.
- `nanocodex` remains a thin Alloy-style facade.
- The local Codex checkout and parity ledger remain the evidence for
  architecture and behavior claims.

### Part 2 — Retained VM foundations

PR #58 added the VM machinery used by interactive agents and owned
evaluation:

- retained libkrun workspaces and VM-backed workspace tools;
- typed guest control, bounded process cleanup, image preparation, and cache
  lifecycle;
- explicit opt-in VM use from one-shot and interactive Nanocodex consumers;
- a dedicated VMM process boundary suitable for Linux hosts and macOS
  development.

Part 2 supplies isolation primitives. It does not make Harbor the evaluation
runner and it does not by itself claim a host-saturating benchmark scheduler.

### Part 3 — VM evaluation, Codex differential parity, and fast experiments

Part 3 is a new first-class evaluation feature implemented by composing
`nanocodex-agent` and `nanocodex-vm` behind the builders in
`nanocodex-eval`, then exposing those builders through the complete
`nanocodex eval ...` CLI. `nanocodex-eval` owns task loading, scheduling,
execution, verification, durable records, aggregation, and comparison.
Harbor-compatible JSONL/ATIF and archive comparison are presentation and
interoperability formats only; no Harbor runner participates in execution.

The binary is a real downstream consumer of that API. `nanocodex-eval`
exposes owned, typed builders for deliberate run policy and keeps image
materialization, network discovery, task-environment maps, verifier caches,
matched backends, stock-Codex guest execution, live capture, and comparison
state private. Common golden-path types are reexported at the crate root;
detailed VM and differential components retain canonical `vm` and
`differential` module paths. Clap, central auth/model flag resolution,
observability installation, process build metadata, terminal rendering, and
exit-code policy stay in the binary.

Local retained state has one current schema. The evaluator does not expose
deprecated aliases, decode old nanoeval markers/configs/results, wildcard-match
missing manifest identity, or upgrade a personal run directory in place.
Resume requires an exact current manifest; rerunning a completed job requires
the current invocation record. An outdated directory is ignored for resume or
fails with an instruction to start a new job. Compatibility decoding is
confined to the separate reader for Harbor's externally published archive.

Every benchmark attempt uses a VM-backed task environment. Host execution is
not a benchmark mode and is not exposed by the eval CLI; it may exist only as
a focused library test fixture. The Nanocodex driver may remain in the host
process while its workspace tools and verifier execute in the guest. The
released stock-Codex binary executes inside its own matched guest.

#### 1. Complete command surface

Support both ordinary Nanocodex evaluation and differential evaluation from
one executable:

- `nanocodex eval` runs one or more tasks or suites with configurable trials,
  retries, concurrency, memory limits, retained jobs, resume, VM image/runtime
  policy, and agent settings.
- `nanocodex eval prepare`, `task`, `inspect`, `compare`, and `cleanup` manage
  task inputs and exact retained evidence.
- `nanocodex eval vm ...` exposes the low-level image/VMM diagnostic boundary.
- `nanocodex eval diff` accepts tasks or suites and starts paired Nanocodex
  and stock-Codex attempts for every selected task, configuration, and trial.
  Its ordinary differential default is k=5, with explicit concurrency and
  pair-memory limits; a one-off k=1 run is an opt-in diagnostic.

The evaluator must retain exact task and verifier revisions, executable
digests, model metadata, tool mode, effort, prompts, tool definitions, ordered
events, API traffic, trajectories, verifier evidence, token/cache usage, cost,
and phase timing. Benchmark tasks and verifiers are immutable inputs.

#### 2. Streaming differential loop

Paired implementations start concurrently, use the same `gpt-5.6-sol`
reasoning effort, and expose progress as it arrives. The differ should identify
the first meaningful divergence without waiting for both attempts to time out
or finish.

Compare more than final answers and scores:

- complete instructions and environment injection;
- outer and nested tool definitions, descriptions, schemas, and ordering;
- every Responses request and visible response event;
- reasoning summaries and encrypted reasoning items observable at the API;
- tool calls, arguments, outputs, timing, errors, and process cleanup;
- typed history deltas, `previous_response_id`, reconnect replay, compaction,
  retry, and cancellation behavior;
- stable prompt-cache identity, byte-stable prefixes, cached-input tokens, and
  model-call/token counts;
- terminal verifier result, latency breakdown, and estimated cost.

The live view must make stalls obvious and distinguish a still-running model
call, tool work, verifier work, retry/backoff, lost guest process, and completed
attempt. Full raw evidence remains on disk even when the terminal view shows a
compact semantic diff.

#### 3. Mode-controlled parity program

The initial Terminal-Bench 2.1 program pins both implementations to
`code_mode_only`. First establish a complete task inventory and work through it
task by task, maintaining a running on-disk log of outcomes and diagnosed
differences.

The complete 89-task inventory may begin with k=1 to find failures and choose
high-signal tasks, but it is discovery evidence rather than a performance
conclusion. Score, mode, effort, and loop-policy decisions use at least k=5 for
every selected `(task × implementation × tool mode × effort)` cell. The
evaluator and CLI preserve those trial coordinates explicitly and default
differential sweeps to five independent attempts.

After that baseline:

1. compare stock Codex in its normal Code Mode configuration against stock
   Codex forced to `code_mode_only`, holding model, effort, task, and repetition
   constant; select the stock treatment explicitly with
   `nanocodex eval diff --codex-tool-mode code-mode` or
   `--codex-tool-mode code-mode-only`;
2. determine whether direct model-visible tools outside Code Mode improve
   success, cost, latency, or robustness;
3. implement that mixed tool exposure in Nanocodex only if the controlled stock
   Codex result demonstrates a benefit;
4. rerun the Nanocodex-versus-Codex differential matrix at each supported
   effort.

Do not infer a loop change from one successful trajectory. Retain repetitions
and classify whether a difference is prompt/context, cache/transport, model
sampling, tool execution, verifier interaction, or scheduler contention.

#### 4. Host-saturating execution

The primary throughput target is `ssh ubuntu@dev-georgios`. Turbo evaluation
must keep the host busy across many Terminal-Bench tasks, efforts,
implementations, modes, and repetitions without multiplying idle VM memory by
every matrix coordinate. VM isolation is invariant in both low-contention and
Turbo runs; Turbo changes allocation and admission, not the trust boundary.

The target allocation unit is one task-worker VM per active benchmark task.
Within it, each
`(implementation × tool mode × effort × repetition)` coordinate runs in a
fresh isolated tenant:

- immutable task lower image plus a private per-attempt overlay;
- private user, mount, PID, IPC, UTS, and network namespaces;
- a private process tree, workspace, ports, temporary paths, outputs, and
  verifier evidence;
- cgroup-v2 CPU, memory, process, and I/O bounds;
- a separate verifier tenant or mount boundary that the agent cannot inspect.

Use a mature Linux isolation runtime for those tenant boundaries rather than
inventing a sandbox. Tasks that need kernel-global privileges or cannot meet
the shared-worker isolation contract fall back to an exclusive VM.

A memory-weighted, work-conserving host scheduler admits only as many task
workers as available RAM permits, fills each with runnable coordinates, and
backfills freed slots immediately. It records queueing, admission, boot,
readiness, warm agent, model, tool, verifier, and cleanup time independently.
Saturated throughput runs and low-contention latency runs are separate modes:
contended wall time is useful capacity evidence but not clean per-agent latency
evidence.

The throughput gate is representative retained evidence that the scheduler
saturates CPU/network capacity on the target host, respects memory limits,
drains and resumes without losing attempt cardinality, and uses materially less
VM overhead than one VM per coordinate.

#### 5. Experiment layer after parity

Once the baseline loop is understood, the same retained evaluator should make
agent experiments cheap:

- PR #32-style recursive/RLM task tools remain an optional consumer layered on
  the owned session API, not a second core scheduler;
- TACT-style trajectory analysis can label overthinking, overacting, and
  calibrated steps from retained traces;
- context, cache, compaction, tool-exposure, and reasoning-effort experiments
  use the same task inputs and differential artifacts.

The goal is a fast cycle: launch a bounded sweep, observe live drift, inspect
the exact paired evidence, make one justified loop change, and rerun only the
coordinates needed to test it. Retaining and displaying a difference is not
the endpoint: repeated Codex advantages must become a Nanocodex loop,
context/cache, or tool improvement (with a focused regression test), while
repeated Nanocodex advantages are preserved.

#### Part 3 completion gates

Part 3 is complete when:

- the full CLI surface compiles and has focused deterministic tests;
- ordinary VM eval and paired VM Codex diff both pass a fresh local smoke from
  `master`;
- the complete Terminal-Bench 2.1 task list and running comparison log are
  retained on disk;
- a controlled `code_mode_only` matrix can run across tasks and reasoning
  efforts with exact paired artifacts;
- repeated stock-Codex advantages have an evidence-backed diagnosis and either
  a verified Nanocodex improvement or an explicit external/policy boundary;
- the differ reports request, response, context/cache, tool, trajectory, and
  verifier divergence while attempts run;
- interrupted and resumed sweeps preserve exact cardinality and partial
  evidence;
- the task-worker isolation contract has an executable regression gate; and
- a representative Turbo run demonstrates bounded, host-saturating execution
  on `dev-georgios`.

Current status (2026-07-29): PR #58 is merged. Draft PR #61 now integrates the
owned evaluator, Harbor-compatible presentation, complete `nanocodex eval`
command tree, stock-Codex capture proxy, paired runner, semantic differ, task
inventory, and running log on current `master`. The paired lifecycle and VM
resource preparation now live behind `nanocodex-eval` builders; the
`nanocodex eval diff` module is only argument translation and rendering rather
than a second evaluator implementation. Local retained schemas are
current-only, with no old-run aliases, inference, or upgrade path. After
integrating
`origin/master` through `a2242a26`, a fresh local smoke from implementation
merge `47b236d11c6addee64f2885c5909560b47069f4a` passed both ordinary
Nanocodex evaluation and concurrent matched-profile Codex differential
evaluation through automatically built, commit-bound guest runtimes. The
schema-v5 differ exposed both the first request drift and a one-turn outer-loop
variance live while preserving exact API, ATIF, event, and verifier evidence.
The first-sample medium-effort `code_mode_only` baseline is complete for all 89
Terminal-Bench 2.1 tasks: Nanocodex scores 79/89 and stock Codex 67/89. That
inventory is k=1 discovery evidence, not a repeated performance result.
API-comparison schema v12 fingerprints every initial model-input text section
and nested Code Mode definition and preserves total usage reconstructed from
captured API responses even when a timed-out stock process has no terminal
usage summary. Subsequent parity gates found and corrected
two model-visible VM context defects: the shell must come from the guest's
UID-0 account, and the date/timezone must come from the guest rootfs rather
than the host-resident agent process. The shell-corrected `0c135a8` cohorts
have profile-valid k=5 cells for `pytorch-model-recovery`, `raman-fitting`, and
`dna-insert` in both stock tool modes. The guest-time-corrected `e8a4593`
cohorts are filling `extract-elf` to k=5; every older ELF sample is excluded
from mode comparison.

The current per-attempt VM adapter has sustained a 48 GiB declared guest
budget. The eval-owned multi-task differential scheduler now defaults to k=5,
preserves task/trial and queue coordinates, charges both arms when a pair
starts, releases each arm's memory charge after evaluator and VM cleanup, and
stages the stock release once per sweep. The initial retained production run
demonstrated that pair-lifetime charging stranded capacity during long
unpaired tails; the per-arm release closes that gap without weakening paired
starts or VM isolation. The pinned remote release and a deterministic
three-task backfill smoke validate the new admission behavior on
`dev-georgios`. A production k=5 cohort then admitted a waiting pair within
124 microseconds of two completed arms returning enough pooled memory while
their original comparisons remained live. The lower-overhead task-worker
allocation described above is not implemented, so no final reduced-VM-overhead
claim is complete.
The CLI now treats `--max-memory-mb` as a hard per-process safety boundary:
it rejects a task whose two declared arms exceed that value instead of relying
on the library scheduler's work-conserving oversized-task exception. It also
reuses the standard eval interrupt machinery so the first Ctrl-C closes
admission and drains already admitted comparisons, while a second Ctrl-C
forces cancellation. Operators still have to partition one host-wide budget
across concurrently running mode processes; cross-process admission is not
implemented. The live campaign demonstrated the exact remaining hazard: an
operator initially counted an 8,192 MiB-per-arm MTEB task as a 4,096 MiB pair
and launched two additional mode processes. The first pair in each process
was cancelled and excluded from scores after briefly taking the host from
48 to 56 GiB of declared live-arm memory. A host-wide scheduler must make
that over-admission structurally impossible rather than relying on manual
arithmetic.
The typed `CodexToolMode` policy and `--codex-tool-mode` selector are
implemented, and the normal-Code-Mode versus Code-Mode-Only experiment is
active; across the latest cells for 42 controlled tasks Nanocodex is 159/210
in the normal-stock cohort and 152/210 in the Code-Mode-Only-stock cohort,
normal stock Codex is 162/210, and Code-Mode-Only stock Codex is 173/210.
`gcode-to-text` is the clearest completed Code-Mode-Only advantage: stock is
2/5 with direct outer tools and 5/5 in Code-Mode-Only. `regex-chess` is the
clearest counterexample at 5/5 with direct outer tools versus 3/5 in
Code-Mode-Only. Nanocodex is 3/5 versus 4/5 on G-code and 4/5 in both
independent Regex cohorts. CompCert is 5/5 for every arm in both modes:
normal outer tools have a stock-specific token/roundtrip benefit after
controlling for the independent Nanocodex sample, but no score or relative
wall-time win. The additional `build-pmars`, `mailman`,
`schemelike-metacircular-eval`, and `mteb-leaderboard` cells show no score
benefit from direct outer tools; Mailman, Scheme, MTEB, and `tune-mjcf`
instead favor Code-Mode-Only on stock efficiency, with MTEB also moving from
4/5 to 5/5. `circuit-fibsqrt` adds another all-axis stock-efficiency win for
Code-Mode-Only, while `pytorch-model-cli` reverses its original k=1
Nanocodex latency regression and has mixed mode-efficiency deltas.
`git-leak-recovery` and `distribution-search` both remain 5/5 everywhere and
add large all-axis stock-efficiency wins for Code-Mode-Only;
`polyglot-c-py` and `fix-git` do the same. `large-scale-text-editing` also
remains 5/5 everywhere and adds a smaller all-axis stock-efficiency win for
Code-Mode-Only. `query-optimize` is 5/5 for Nanocodex and 4/5 for stock in
both modes; the stock-only failures are correct-but-slow generated SQL, while
large independent-control variance prevents an all-axis mode claim.
`custom-memory-heap-crash` remains 5/5 everywhere and adds another clear
all-axis stock-efficiency win for Code-Mode-Only.
`adaptive-rejection-sampler` also remains 5/5 everywhere. Its repeated
post-verifier-validation tails expose a stopping-policy difference rather than
a Nanocodex loop regression, and direct outer tools add no score.
Fresh `dna-assembly` k=5 cells reverse the prior normal-mode winner from
0/5-versus-2/5 to 3/5-versus-0/5 without a runtime change; every failure is
the same generated-primer Tm/annealing strategy boundary, with healthy
context, cache, chains, and polling.
That is directional evidence for Code-Mode-Only, not yet a broad causal
winner: each mode uses independent model samples, and repeated tasks continue
to show substantial within-configuration variance.

The API differ also exposed a remaining non-model-visible request-envelope
drift in Responses `client_metadata`. Commit `d3d01b7d` now preserves the same
installation, session/window, logical-turn, and per-send timestamp lifetimes
as pinned stock Codex while retaining the newer Lite-only Code Mode tool-name
metadata. Focused API and agent tests, warnings-denied Clippy, rustfmt, and
crate-boundary checks pass, and an exact remote binary is staged as a new
cohort. Its VM smoke and k=5 affected-task reruns remain pending capacity; it
will not be mixed into the active older cohorts.

## Current execution order

1. [x] Merge the stable agent/API refactor and retained VM foundation.
2. [x] Integrate `nanocodex-eval` and the complete `nanocodex eval ...` CLI on
   current `master`.
3. [x] Rerun a local VM smoke and one paired `code_mode_only` Terminal-Bench
   2.1 task; inspect exact JSONL, ATIF, API capture, trajectory, and verifier
   output.
4. [x] Strengthen the live differ wherever that evidence exposes an ambiguous
   or late diagnosis.
5. [x] Run the first-sample Terminal-Bench 2.1 `code_mode_only` differential
   baseline on `dev-georgios` and maintain the complete on-disk task log.
6. [ ] Make first-turn context equivalent, repeat score/cost outliers, and turn
   stable stock-Codex advantages into focused Nanocodex improvements.
7. [ ] Implement and gate task-worker tenant isolation and the memory-weighted
   work-conserving scheduler.
8. [ ] Run stock Codex normal Code Mode versus `code_mode_only`; adopt mixed
   tool exposure only if the controlled result is better.
9. [ ] Rerun the winning configuration across supported efforts, then layer
   RLM and trajectory-labeling experiments on the proven evaluator.

## Current non-goals

- No provider abstraction, generic app server, compatibility layer, approval
  subsystem, or alternate agent runtime.
- No Harbor-owned execution path; Harbor remains a compatible record and ATIF
  boundary.
- No benchmark, task, or verifier modification made to improve an eval score.
- No claim that saturated wall time is uncontended per-agent latency.
- No generic multi-agent scheduler in the stable agent crates.
- No browser, managed-agent, or Tempo-specific dependency in public
  `nanocodex-*` crates as part of this evaluation slice.
