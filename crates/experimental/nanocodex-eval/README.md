# nanocodex-eval

`nanocodex-eval` is the owned benchmark lifecycle for Nanocodex. It composes
`nanocodex-agent` with `nanocodex-vm`, schedules task × agent × repetition
sweeps, runs canonical verifiers, and retains exact events, API evidence,
trajectories, timing, usage, and results.

Harbor-compatible JSONL and ATIF are output formats. Harbor does not run the
task, agent, VM, or verifier.

Local eval artifacts use one current schema. Resume requires an exact current
run manifest, and rerun requires the current invocation record. The crate does
not decode or upgrade old nanoeval run directories; start a new job instead.
The published-Harbor reader is a separate external-interoperability boundary.

## VM backend

The benchmark CLI always uses a microVM-backed task environment. The
Nanocodex driver stays in the host process while its workspace tools execute
in the guest; a stock Codex comparison binary executes inside a separate
matched guest. The native backend remains only as a small library test fixture.

Applications install VM execution through the eval facade:

```rust,ignore
use nanocodex_agent::{Nanocodex, OpenAi};
use nanocodex_eval::{
    Evaluator, Task, VmResources,
    vm::VmBackend,
};

let task = Task::load("terminal-bench/tasks/example")?;
let agent = Nanocodex::builder(OpenAi::new(std::env::var("OPENAI_API_KEY")?)?);
let resources = VmResources::builder(
    "target/debug/nanocodex",
    ".cache/vm/runtime.ext4",
)
    .task(task.clone())
    .prepare()
    .await?;
let backend = VmBackend::builder()
    .web_search(false)
    .retain_passed_rootfs(false)
    .build();
resources.configure(&backend).await?;

let (evaluator, events) = Evaluator::builder(agent)
    .output_directory(".nanocodex/evals")
    .max_concurrency(8)
    .max_memory_mb(32_768)
    .vm(backend)
    .build()?;
let _ = events;
let result = evaluator.task(task).await?;
```

`EvaluatorBuilder::vm` sets the durable environment identity to `micro_vm`
itself. A caller cannot install this backend while accidentally recording the
attempt as native.

`VmResources` owns OCI image materialization, public-network helper discovery,
task-to-environment mapping, and verifier-cache preparation. The detailed
`VmBackendConfiguration` and `VmEnvironment` types remain available under
`nanocodex_eval::vm` for custom runtimes, but the normal evaluator and
differential paths do not assemble them.

## Differential runner

`DifferentialEvaluator` owns the reusable matched two-arm lifecycle and
memory-weighted pair admission. The binary supplies the already configured
Nanocodex recipe, shared auth selection, one prepared VM resource set, and
executable identities:

```rust,ignore
use nanocodex_eval::{
    CodexAuth, DifferentialEvaluator, ExecutableIdentity, Task, VmResources,
};

let tasks = vec![
    Task::load("terminal-bench/tasks/example-a")?,
    Task::load("terminal-bench/tasks/example-b")?,
];
let vm = VmResources::builder("nanocodex", "runtime.ext4")
    .tasks(tasks.clone())
    .prepare()
    .await?;
let reports = DifferentialEvaluator::builder(nanocodex)
    .codex("codex-linux", CodexAuth::auth_file("~/.codex/auth.json"))
    .vm(vm)
    .thinking(thinking)
    .web_search(false)
    .nanocodex_executable(ExecutableIdentity::new("nanocodex", version))
    .max_concurrency(8)
    .max_memory_mb(49_152)
    .build()?
    .tasks_n(tasks, 5)
    .await?;
```

The library stages the Codex release once per evaluator, admits each pair
against both arms' declared memory, releases each arm's charge after its
evaluator and VM cleanup finish, creates matched isolated backends, runs both
arms concurrently, streams the live divergence record, projects ATIF, compares
API event loops, and returns typed retained reports with explicit one-indexed
trial coordinates. Clap, observability installation, process build metadata,
terminal formatting, and exit-code policy stay in the binary.

## CLI

Run Nanocodex normally over one task or a complete suite:

```sh
nanocodex eval \
  --suite /data/terminal-bench-2.1/tasks \
  --trials 5 \
  --concurrency 24 \
  --max-memory-mb 98304 \
  --thinking medium \
  --web-search false
```

Run a k=5, paired, concurrent `code_mode_only` sweep against a released Linux
Codex binary:

```sh
nanocodex eval diff \
  --suite /data/terminal-bench-2.1/tasks \
  --codex-bin /opt/codex/codex-x86_64-unknown-linux-musl \
  --concurrency 24 \
  --max-memory-mb 49152 \
  --thinking medium \
  --web-search false
```

`eval diff` defaults to five independent matched pairs per task. Pass
`--trials 1` only for a one-off diagnostic.

### StableBench v1

StableBench's checked-in tasks do not contain the pinned Tempo documentation
sidecar. The Tempo harness stages that immutable input immediately before a
run. To reproduce the suite revision introduced with StableBench v1, first
stage it from the Tempo evaluator checkout (the helper also fetches the docs
revision in `config/tempo-docs.lock.json`):

```sh
git -C /data/tempo-evals checkout 45d044ef
cd /data/tempo-evals
uv sync
uv run python - <<'PY'
from pathlib import Path
import shutil

from scripts.run_benchmark import docs_source, ensure_docs_bundle, stage_task_datasets

root = Path(".cache/nanocodex-stable-bench-v1")
shutil.rmtree(root, ignore_errors=True)
stage_task_datasets(root, ensure_docs_bundle(docs_source({})))
print((root / "tasks" / "tempo-v1").resolve())
PY
```

Use the printed directory as Nanoeval's suite. The Docs treatment gives both
arms the staged `docs.tempo.xyz` snapshot. The v1 GHCR images are amd64-only,
so run this revision on an x86_64 eval host:

```sh
nanocodex eval diff \
  --suite /data/tempo-evals/.cache/nanocodex-stable-bench-v1/tasks/tempo-v1 \
  --codex-bin /opt/codex/codex-x86_64-unknown-linux-musl \
  --stable-bench-v1 \
  --trials 5
```

On Apple Silicon, build the same image sources for Linux arm64 and publish
them through a loopback registry. Nanoeval resolves loopback HTTP registries,
so the images do not need to be published externally:

```sh
cd /data/tempo-evals
docker run -d --name nanoeval-registry -p 5100:5000 registry:2
docker build --platform linux/arm64 \
  --tag localhost:5100/stable-bench/agent:v1-arm64 \
  --file shared/global/docker/agent/Dockerfile .
docker build --platform linux/arm64 \
  --build-arg AGENT_IMAGE=localhost:5100/stable-bench/agent:v1-arm64 \
  --tag localhost:5100/stable-bench/verifier:v1-arm64 \
  --file shared/global/docker/verifier/Dockerfile .
docker push localhost:5100/stable-bench/agent:v1-arm64
docker push localhost:5100/stable-bench/verifier:v1-arm64
```

Rewrite only the generated task copies to those architecture-equivalent
images, leaving the checked-in benchmark and verifier untouched:

```sh
uv run python - <<'PY'
from pathlib import Path

suite = Path(".cache/nanocodex-stable-bench-v1/tasks/tempo-v1")
images = {
    "environment": "localhost:5100/stable-bench/agent:v1-arm64",
    "tests": "localhost:5100/stable-bench/verifier:v1-arm64",
}
for role, image in images.items():
    for dockerfile in suite.glob(f"*/{role}/Dockerfile"):
        lines = dockerfile.read_text().splitlines()
        index = next(i for i, line in enumerate(lines) if line.startswith("FROM "))
        lines[index] = f"FROM {image}"
        dockerfile.write_text("\n".join(lines) + "\n")
PY
```

The stock arm must also be a Linux arm64 Codex release, rather than the macOS
host executable. For example, extract the platform package for the release
being compared and pass its vendored ELF. Nanoeval also stages and hashes the
package's adjacent `codex-code-mode-host` when it is present:

```sh
mkdir -p /tmp/nanoeval-codex-linux-arm64
cd /tmp/nanoeval-codex-linux-arm64
npm pack '@openai/codex@0.146.0-linux-arm64'
tar -xzf openai-codex-0.146.0-linux-arm64.tgz
CODEX_BIN=$PWD/package/vendor/aarch64-unknown-linux-musl/bin/codex

cd /path/to/nanocodex
ulimit -n 10240
cargo run -p nanocodex-bin -- eval diff \
  --task /data/tempo-evals/.cache/nanocodex-stable-bench-v1/tasks/tempo-v1/transfer-with-memo \
  --codex-bin "$CODEX_BIN" \
  --stable-bench-v1 \
  --guest-memory-mb 4096 \
  --trials 1 \
  --concurrency 1 \
  --prepare-concurrency 1
```

Four GiB avoids the Node/npm heap pressure observed in this task's install and
typecheck cycle. Replace `--task` with `--suite` and raise the scheduler limits
for a complete sweep.

Add the exact Tempo MCP endpoint to both arms for the MCP treatment:

```sh
nanocodex eval diff \
  --suite /data/tempo-evals/.cache/nanocodex-stable-bench-v1/tasks/tempo-v1 \
  --codex-bin /opt/codex/codex-x86_64-unknown-linux-musl \
  --stable-bench-v1 \
  --stable-bench-mcp \
  --trials 5
```

Nanoeval runs the task-owned deterministic onchain verifier and records its
binary `correctness` reward. A fresh, tool-less Nanocodex session then grades
only the rubric-declared submission files and emits `quality`; failed
correctness skips the model judge and forces quality to zero. The task's
RewardKit/Anthropic quality judge is never invoked. The result retains the
complete verifier logs, the pre-verification ATIF trajectory, judge events,
raw judge output, and normalized named rewards for both arms.

Both agents default to `code_mode_only`. To run normal Code Mode on both arms,
select it explicitly:

```sh
nanocodex eval diff \
  --suite /data/terminal-bench-2.1/tasks \
  --codex-bin /opt/codex/codex-x86_64-unknown-linux-musl \
  --nanocodex-tool-mode code-mode \
  --codex-tool-mode code-mode
```

Mode lists pair positionally, with a singleton broadcast across the other
side. A two-treatment sweep therefore runs each of the four implementations
once per task and trial:

```sh
nanocodex eval diff \
  --suite /data/terminal-bench-2.1/tasks \
  --codex-bin /opt/codex/codex-x86_64-unknown-linux-musl \
  --nanocodex-tool-mode code-mode,code-mode-only \
  --codex-tool-mode code-mode,code-mode-only
```

Both commands use the same central CLI auth and model flags. Authentication
selection is, in order: `--api-key`, `--auth-file`, the default Codex auth
file, `OPENAI_API_KEY`, then the default auth-file path. Normal agent sessions
retain their standard defaults; eval defaults to medium thinking with web
search disabled unless the flags or a resumed invocation say otherwise.

The remaining command surface manages the same retained evidence:

```sh
nanocodex eval prepare --suite /data/terminal-bench-2.1/tasks
nanocodex eval task /data/terminal-bench-2.1/tasks/example --prompt
nanocodex eval inspect .nanocodex/evals/JOB --full
nanocodex eval compare .nanocodex/evals/JOB
nanocodex eval cleanup .nanocodex/evals/JOB --dry-run
nanocodex eval vm --help
```

`eval diff` writes interleaved live progress while both lanes execute and
retains the complete comparison, API exchanges, raw streams, derived ATIF,
verifier output, and final workspaces for offline reanalysis.

The ATIF summaries label their projection scope. Nanocodex lifecycle events
contain both the model-visible Code Mode call and its nested tools, while the
stock CLI stream exposes completed inner items. Those raw tool counts remain
available but are not treated as directly comparable. The API event-loop
comparison separately records the exact model-visible tool-call sequence for
both arms and uses that sequence for parity claims.
