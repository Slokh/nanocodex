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
- agent-owned process lifetime at the agent/verifier boundary: managed
  foreground commands must be terminated for both implementations, while
  children that deliberately detach from the managed process group may
  survive into same-guest verification;
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
Quarantine a cell from agent-parity totals when the two adapters do not apply
the same cleanup boundary. A service that survives only because an
evaluator-owned remote tool runtime outlives one agent is runner evidence, not
agent-quality evidence.

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
- stock and Nanocodex apply the same managed-process cleanup boundary before
  verification, with a regression proving that foreground commands die and
  deliberately detached services survive on both arms;
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
cohorts completed profile-valid `extract-elf` cells in both modes; every older
pre-correction ELF sample remains excluded from mode comparison.

The current per-attempt VM adapter has sustained a 48 GiB declared guest
budget. The eval-owned multi-task differential scheduler now defaults to k=5,
preserves task/trial and queue coordinates, charges both arms when a pair
starts, releases each arm's memory charge after evaluator and VM cleanup, and
stages the stock release once per sweep. Here k=5 means five valid independent
matched pairs per task, not five launches regardless of evaluator health.
The process-wide runner now accepts repeated reasoning-effort and stock
tool-mode profiles in one invocation and expands
`(task × effort × tool mode × repetition)` through one central completion
queue, admission controller, prepared-image map, and staged stock release.
Pair capacity is represented as two active-arm slots; each completed arm
returns one slot and its own learned host-RSS estimate, so two unrelated
completed arms can backfill another matched pair while their former
counterparts remain live. Task images are prepared lazily through bounded
single-flight cells: each task enters admission as its image resolves while
other image work continues, and a failed image blocks only its task coordinates.
The immutable image is shared by matched arms while writable disks, sessions,
and verifier state remain private.

Differential VM arms now start from a low eval-only guest allocation, sample
guest peak use and OOM counters plus VMM host RSS, and persist task-content-
keyed sizing with slack in the VM cache. A confirmed OOM retains an unscored
attempt and geometrically requeues both arms of the same logical trial up to
the task declaration; ordinary model loss never changes memory. Separate
Nanocodex and stock host estimates drive admission and are released with their
arms. A sweep manifest and exclusive output lock make the full matrix safely
resumable without mixing tasks, profiles, trials, models, or executable builds.
Explicit `nanocodex eval prepare` remains available independently.
Comparison schema v12 and the evaluator builder now retain infrastructure-
broken pairs, schedule bounded replacements at fresh trial coordinates, and
link every replacement to the failed trial it supersedes. The CLI budgets up
to one extra k-sized cell per task and fails after writing all evidence if it
still cannot obtain k valid pairs; verifier failures, timeouts, refusals, and
ordinary model losses are never retried. Every completed CLI sweep also emits
one typed per-task score line with valid/target pairs, total attempts,
infrastructure and incomplete counts, and exact Nanocodex/stock pass
numerators, so operators do not infer a cell from terminal event order. The
initial retained production run
demonstrated that pair-lifetime charging stranded capacity during long
unpaired tails; the per-arm release closes that gap without weakening paired
starts or VM isolation. The pinned remote release and a deterministic
three-task backfill smoke validate the new admission behavior on
`dev-georgios`. A production k=5 cohort then admitted a waiting pair within
124 microseconds of two completed arms returning enough pooled memory while
their original comparisons remained live. The lower-overhead task-worker
allocation described above is not implemented, so no final reduced-VM-overhead
claim is complete.
The earlier largest host-utilization loss was before admission: a cold 33-task
process took about 6.5 minutes to prepare every selected image before its first
comparison, while warm semantic keys resolved in under two seconds. Lazy
bounded preparation now removes that batch barrier. `--max-memory-mb` is a
process-wide measured-host-memory target; an estimate above it runs alone, and
`--guest-memory-mb` selects the initial per-arm guest allocation rather than a
permanent cap. The CLI also
reuses the standard eval interrupt machinery so the first Ctrl-C closes
admission and drains already admitted comparisons, while a second Ctrl-C
forces cancellation. The supported saturation path is one invocation owning
the complete profile matrix; independently launched processes still do not
share admission. The live campaign demonstrated the old hazard: an
operator initially counted an 8,192 MiB-per-arm MTEB task as a 4,096 MiB pair
and launched two additional mode processes. The first pair in each process
was cancelled and excluded from scores after briefly taking the host from
48 to 56 GiB of declared live-arm memory. A host-wide scheduler must make
that over-admission structurally impossible rather than relying on manual
arithmetic.
The typed `CodexToolMode` policy and `--codex-tool-mode` selector are
implemented, and the normal-Code-Mode versus Code-Mode-Only experiment is
active; across the latest valid cells for 81 controlled tasks Nanocodex is
336/405 in the normal-stock cohort and 334/405 in the Code-Mode-Only-stock
cohort, normal stock Codex is 333/405, and Code-Mode-Only stock Codex is
331/405. The 810-pair pool is 670/810 versus 664/810, with 63
Nanocodex-only and 57 stock-only outcomes (`p=0.6483`, exploratory paired
sign test). Initial model-visible text matches in 809/810 pairs, every
Code-Mode-Only nested catalog matches, cache identities stay stable, all
response/tool-result chains are valid, and the 45 replayed pairs split
exclusive outcomes evenly. The current score difference is therefore not
evidence of a broken Nanocodex event loop.
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
`fix-ocaml-gc` is 5/5 on every arm in its first controlled cells; Nanocodex
is slightly faster and uses fewer total tokens and generation turns in both
stock modes, with healthy model-selected compiler polling.
The deepest current stock-favored trajectories are generated
interpretation/validation gaps. On Video, four stock-only pairs use robust
silhouette/contact or calibrated crossing detectors while Nanocodex chooses a
wrong stride, over-filters the hidden runner, or misses the takeoff upper
bound by one frame. On Raman, stock retains the physically correct reciprocal
axis in 5/10 samples versus Nanocodex in 1/10; all three stock passes are in
that subset, with remaining misses due to fit baseline/width. On cancellation,
Nanocodex's semaphore-plus-`gather` design double-cancels active jobs during
asynchronous cleanup when a queued job exists; stock's bounded `TaskGroup`
workers test and pass that topology. On MTEB retrieval, stock chooses the
required query/passage prompt types in three of four discordant samples,
whereas the loser treats retrieval as symmetric STS encoding. These are
model-selected solution and stopping differences after the first output
divergence, not cache or transport failures.
Three service tasks expose an adapter lifecycle defect. Nanocodex is 29/30
and stock is 4/30 across `hf-model-inference`, `kv-store-grpc`, and
`pypi-server` because the evaluator-owned VM tool session retains Nanocodex
foreground guest commands after `agent.shutdown()`. At the reviewed stock
checkpoint, Codex deliberately terminates all managed Unified Exec processes
during session shutdown; Nanocodex normally terminates its owned shell
sessions too, but the remote guest runtime is caller-owned and the evaluator
reuses it for verification without sending the equivalent cancellation.
These 30 pairs are therefore quarantined from agent-parity interpretation
until the cells are rerun. The evaluator now sends a non-destructive managed
tool-process termination request at the agent/verifier boundary for every arm.
Focused VM regressions prove that a foreground process dies, a process moved
into a separate process group survives, and the same guest remains usable for
verification.
The provisional unaffected-task view is stock 660/780 versus Nanocodex
641/780; it remains descriptive because the lifecycle defect was found after
reading outcomes.
There is still no broad causal tool-mode winner: each mode uses independent
model samples, and repeated tasks continue to show substantial
within-configuration variance.

The API differ also exposed a remaining non-model-visible request-envelope
drift in Responses `client_metadata`. Commit `d3d01b7d` now preserves the same
installation, session/window, logical-turn, and per-send timestamp lifetimes
as pinned stock Codex while retaining the newer Lite-only Code Mode tool-name
metadata. Focused API and agent tests, warnings-denied Clippy, rustfmt, and
crate-boundary checks pass. An exact remote binary passed a fresh excluded
k=1 deployment/metadata smoke on both arms; its raw captures validate the
ported identity lifetimes. Fresh matched k=5 affected-task cohorts are now
running and are not mixed with older runners.
Two stock arms in a later normal-Video repetition then exposed an independent
eval-transport failure: guest DNS stopped resolving `host.containers.internal`
while the host capture proxy remained alive. Commit `75bc9fac` now uses
gvproxy's owned direct host-loopback route instead of DNS. Focused and complete
eval/VM tests, doc tests, warnings-denied Clippy, formatting, and boundary
checks pass. An exact remote release completed a fresh excluded k=1
connectivity smoke through `192.168.127.254`; both arms passed with a complete
stock capture and verifier result. A same-cause DNS failure observed in a
still-running older broad cohort caused every d3 cohort to stop admission and
drain, preserving its partial evidence. Fresh `75bc9fac` broad queues and
matched normal/Code-Mode-Only Video repetitions are now running from new
roots.

The latest valid table now covers 81 tasks; 8/89 still lack at least one
complete k=5 mode cell and remain outside the denominator. At
2026-07-29 20:51 UTC, the retained broad queues contain 154 normal-mode and
156 Code-Mode-Only comparisons, with their remaining partial tasks still
running. The heavy-task lane and both fresh `train-fasttext` mode cells are
also active. The five admitted eval processes again sum to the exact 48 GiB
configured future ceiling.

The broad launch exposed two separate image-startup costs. First, eager image
materialization left a cold large sweep idle before admission; the central
runner now resolves each task lazily with bounded single-flight preparation.
Second, the build-cache key hashed the complete evaluator
executable because that executable also hosts the `vm-run-config` entry point.
Any unrelated agent, capture, reporting, or evaluator revision therefore
invalidated every Dockerfile-built task image even when VM build semantics and
all task inputs were unchanged. `VmImageBuilder` now retains executable-digest
keying as the safe default but permits an embedding application to supply an
explicit semantic VMM identity. The evaluator pins that identity to its narrow
VM-process contract, whose arguments, runtime, firmware, resources, network,
resolver, and egress inputs remain independently keyed. A focused regression
proves that the default identity changes with executable bytes, the explicit
identity survives unrelated bytes, and an explicit version bump invalidates
it. Complete VM and eval tests and doc tests pass, and the exact release is
staged on `dev-georgios`. This removes revision-wide cold-cache churn after
one intentional namespace transition; lazy scheduling now also removes the
generic cold batch barrier.

The first clean direct-IP Video repetition exposed a separate transient
whole-gvproxy-route loss after several minutes of successful stock work.
Two stock trials lost that route, while the other three completed; the failed
attempts are retained and the incomplete normal-mode cell is excluded.
The prior owner silently discarded gvproxy's exit status, so the current
follow-up appends unexpected early exit status/signal and lifetime to the
attempt log and tracing. This strengthens the next causal diagnosis without
pretending that direct addressing can repair a dead network process.

Task-image preparation has a separate direct-TSI network boundary. A cold
GPT-2 image resolved and downloaded one blob, then failed to resolve the same
host for the next Dockerfile instruction and aborted the whole sweep before
admission. `VmResourcesBuilder` now owns a bounded image-network retry policy,
defaulting to two whole-image retries. Only recognized network failures in a
Dockerfile build step are retried; deterministic build failures still return
immediately. Retrying starts from immutable task inputs and the content cache,
retains the full warning/error in the eval trace, and avoids requiring an
operator to replace a zero-attempt sweep after transient DNS loss.

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
