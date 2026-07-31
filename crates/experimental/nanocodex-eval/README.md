# nanocodex-eval

`nanocodex-eval` owns Nanocodex's VM-isolated benchmark lifecycle: task
loading, bounded scheduling, fresh attempts, canonical verification, resumable
jobs, typed events and outcomes, Harbor projection, aggregation, and matched
Codex comparison.

Every benchmark attempt runs tools and verification in a microVM. Native host
execution exists only inside focused crate tests. Harbor JSONL and ATIF are
output formats, not alternate runners.

## One task

```rust,no_run
use nanocodex_agent::{Nanocodex, OpenAi};
use nanocodex_eval::{Evaluator, Task, VmResources};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let task = Task::load("tasks/write-greeting")?;
let resources = VmResources::builder("nanocodex", "runtime.ext4")
    .task(task.clone())
    .prepare()
    .await?;
let evaluator = Evaluator::builder(
    Nanocodex::builder(OpenAi::new(std::env::var("OPENAI_API_KEY")?)?),
    resources.backend().await?,
)
.output_directory(".nanocodex/evals")
.build()?;

let run = evaluator.task(task);
let mut events = run.events().subscribe();
let observer = tokio::spawn(async move {
    while let Some(event) = events.recv().await? {
        println!("{} {:?}", event.sequence, event.kind);
    }
    Ok::<_, nanocodex_eval::EvalEventStreamError>(())
});
let outcome = run.await?;
observer.await??;
println!("{:?}", outcome.outcome());
# Ok(())
# }
```

`EvalRun<T>` is independently awaitable and owns an optional event stream.
Every event carries a job ID, invocation ID, invocation-wide sequence, and—on
attempt events—typed attempt identity and ordering. Every invocation emits one
terminal event, including cancellation.

## Resumable sweep

Build a `Sweep`, then consume it exactly once with
`resume_incomplete(sweep)` or `fresh_run(sweep)`. The resulting evaluator is
bound to that manifest, so execution is simply `evaluator.sweep()` and cannot
accidentally receive a different workload.

See the compiled examples:

- `eval-task`: one VM attempt, independent events, and Harbor projection.
- `eval-sweep`: a resumable multi-agent sweep.
- `eval-differential`: matched Nanocodex-versus-Codex trials.

Set `NANOCODEX_BIN` and `NANOCODEX_VM_RUNTIME` when the default development
paths do not apply.

## Differential evaluation

Detailed comparison APIs live under `nanocodex_eval::differential`; detailed
VM APIs live under `nanocodex_eval::vm`. `DifferentialEvaluatorBuilder::prepare`
is async because it hashes and stages executables and loads retained memory
profiles before execution.

The scheduler is work-conserving across concurrency and memory limits. A task
larger than the memory target runs alone, differential arms release capacity
independently, image preparation overlaps across tasks, and draining stops new
admission while joining work already admitted.

## CLI

```sh
nanocodex eval --suite /data/terminal-bench/tasks --trials 5
nanocodex eval diff \
  --suite /data/terminal-bench/tasks \
  --codex-bin /opt/codex/codex-x86_64-unknown-linux-musl
```

The CLI installs auth and observability, resolves reusable scheduling and VM
configuration, invokes this library, and renders results. Model execution,
scheduling, verification, persistence, and comparison remain library-owned.

Local retained state has one current schema. Resume requires an exact current
manifest; old run directories are not upgraded in place. Published Harbor
reading remains a separate interoperability boundary.
