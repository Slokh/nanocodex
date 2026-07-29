use std::{
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use clap::{Args, ValueEnum};
use eyre::{Result, eyre};
use nanocodex::Thinking;
use nanocodex_eval::*;

use super::run;
use crate::{
    config::{EvalAgentArgs, SharedAuth},
    observability::ObservabilityArgs,
};

const DEFAULT_OUTPUT_DIRECTORY: &str = ".nanocodex/eval-diff";

#[derive(Clone, Copy, Default, ValueEnum)]
enum StockCodexToolMode {
    /// Expose normal tools directly as well as through Code Mode.
    CodeMode,
    /// Expose normal tools only through Code Mode's `exec` entrypoint.
    #[default]
    CodeModeOnly,
}

impl From<StockCodexToolMode> for CodexToolMode {
    fn from(value: StockCodexToolMode) -> Self {
        match value {
            StockCodexToolMode::CodeMode => Self::CodeMode,
            StockCodexToolMode::CodeModeOnly => Self::CodeModeOnly,
        }
    }
}

#[derive(Args)]
pub(crate) struct Diff {
    /// Rebuild the derived API/event-loop comparison from retained raw captures.
    ///
    /// This performs no agent, model, VM, or verifier work.
    #[arg(
        long,
        value_name = "COMPARISON_DIRECTORY",
        conflicts_with_all = [
            "tasks",
            "suites",
            "codex_bin",
            "vm_cache",
            "vm_guest_runtime",
            "vm_refresh"
        ]
    )]
    reanalyze: Option<PathBuf>,

    /// Evaluator task directory to run through both agents. Repeat for a batch.
    #[arg(
        long = "task",
        value_name = "DIRECTORY",
        required_unless_present_any = ["reanalyze", "suites"],
        conflicts_with = "reanalyze"
    )]
    tasks: Vec<PathBuf>,

    /// Suite whose immediate task children should run through both agents.
    #[arg(
        long = "suite",
        value_name = "DIRECTORY",
        required_unless_present_any = ["reanalyze", "tasks"],
        conflicts_with = "reanalyze"
    )]
    suites: Vec<PathBuf>,

    /// Exact stock-Codex executable to compare against Nanocodex.
    ///
    /// This must be the Linux executable that runs in the guest.
    #[arg(
        long,
        value_name = "EXECUTABLE",
        required_unless_present = "reanalyze",
        conflicts_with = "reanalyze"
    )]
    codex_bin: Option<PathBuf>,

    /// Stock Codex tool exposure used by this controlled comparison.
    #[arg(long, value_enum, default_value = "code-mode-only")]
    codex_tool_mode: StockCodexToolMode,

    /// Parent directory for paired evaluator artifacts.
    #[arg(long, default_value = DEFAULT_OUTPUT_DIRECTORY)]
    output: PathBuf,

    /// Number of independent matched pairs per task.
    #[arg(
        long,
        default_value_t = run::DEFAULT_TRIALS,
        value_parser = clap::value_parser!(u16).range(1..)
    )]
    trials: u16,

    /// Maximum number of matched pairs executing at once.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    concurrency: Option<u16>,

    /// Maximum task-declared memory across both arms of admitted pairs.
    #[arg(long, value_name = "MIB", value_parser = clap::value_parser!(u64).range(1..))]
    max_memory_mb: Option<u64>,

    /// Percentage of detected host CPU and memory used for omitted limits.
    #[arg(
        long,
        default_value_t = run::DEFAULT_HOST_UTILIZATION_PERCENT,
        value_name = "PERCENT",
        value_parser = clap::value_parser!(u8).range(1..=100)
    )]
    host_utilization: u8,

    /// Use this prebuilt Nanocodex guest-runtime ELF.
    #[arg(long, value_name = "ELF")]
    vm_guest_runtime: Option<PathBuf>,

    /// Content-addressed VM cache shared across differential runs.
    #[arg(long, value_name = "DIRECTORY", default_value = ".cache/vm")]
    vm_cache: PathBuf,

    /// Resolve the task image at the registry instead of reusing its local resolution.
    #[arg(long)]
    vm_refresh: bool,

    /// Print the complete comparison record as JSON.
    #[arg(long)]
    json: bool,

    #[command(flatten)]
    observability: ObservabilityArgs,

    #[command(flatten)]
    agent: EvalAgentArgs,
}

impl Diff {
    pub(crate) async fn run(self) -> Result<()> {
        let _observability = self.observability.install(false, Path::new("."))?;
        if let Some(directory) = self.reanalyze {
            let reanalysis = reanalyze(directory)?;
            if self.json {
                write_json(reanalysis.comparison())?;
            } else {
                print!("{}", reanalysis.human_summary());
            }
            return Ok(());
        }

        let tasks = run::load_tasks(self.tasks, self.suites)?;
        let (automatic_concurrency, automatic_memory_mb) =
            run::automatic_scheduling_defaults(self.host_utilization);
        let concurrency = self.concurrency.unwrap_or(automatic_concurrency);
        let max_memory_mb = self.max_memory_mb.or(automatic_memory_mb);
        eprintln!(
            "Differential sweep: {} task(s) × k={} · up to {} pair(s) · {}",
            tasks.len(),
            self.trials,
            concurrency,
            max_memory_mb.map_or_else(
                || "unbounded declared memory".to_owned(),
                |memory| format!("{memory} MiB pair-memory ceiling")
            )
        );
        let thinking = self.agent.thinking().unwrap_or(Thinking::Medium);
        let web_search = self.agent.web_search().unwrap_or(false);
        let (nanocodex, auth) = self.agent.shared_builder(thinking, web_search)?;
        let codex_auth = match auth {
            SharedAuth::ApiKey(api_key) => CodexAuth::api_key(api_key),
            SharedAuth::AuthFile(path) => CodexAuth::auth_file(path),
        };
        let current_executable = std::env::current_exe()?;
        let runtime_image =
            run::prepare_vm_guest_runtime_from(self.vm_guest_runtime.as_deref(), &self.vm_cache)
                .await?;
        let vm = VmResources::builder(&current_executable, runtime_image)
            .tasks(tasks.clone())
            .cache_directory(&self.vm_cache)
            .cache_policy(if self.vm_refresh {
                CachePolicy::Refresh
            } else {
                CachePolicy::Reuse
            })
            .prepare()
            .await?;
        let mut evaluator = DifferentialEvaluator::builder(nanocodex)
            .codex(
                self.codex_bin
                    .ok_or_else(|| eyre!("--codex-bin is required unless --reanalyze is used"))?,
                codex_auth,
            )
            .vm(vm)
            .output_directory(self.output)
            .thinking(thinking)
            .web_search(web_search)
            .codex_tool_mode(self.codex_tool_mode.into())
            .nanocodex_executable(
                ExecutableIdentity::new(current_executable, env!("NANOCODEX_SEMVER_VERSION"))
                    .git_sha(env!("VERGEN_GIT_SHA"))
                    .built_at(env!("VERGEN_BUILD_TIMESTAMP")),
            )
            .max_concurrency(usize::from(concurrency));
        if let Some(max_memory_mb) = max_memory_mb {
            evaluator = evaluator.max_memory_mb(max_memory_mb);
        }
        let evaluator = evaluator.build()?;
        let reports = evaluator.tasks_n(tasks, usize::from(self.trials)).await?;

        if self.json {
            write_json(&reports)?;
        } else {
            for report in &reports {
                print!("{}", report.human_summary());
            }
        }
        let operational_errors = reports
            .iter()
            .filter(|report| report.has_operational_error())
            .collect::<Vec<_>>();
        if !operational_errors.is_empty() {
            return Err(eyre!(
                "{} comparison runner(s) failed; first evidence retained at {}",
                operational_errors.len(),
                operational_errors[0].comparison_path().display()
            ));
        }
        Ok(())
    }
}

fn write_json(value: &impl serde::Serialize) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser;

    use super::Diff;
    use crate::eval::run::DEFAULT_TRIALS;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        diff: Diff,
    }

    #[test]
    fn differential_cli_defaults_to_k_five() {
        let cli = TestCli::try_parse_from([
            "nanoeval",
            "--task",
            "tasks/first",
            "--codex-bin",
            "/opt/codex",
        ])
        .unwrap();

        assert_eq!(cli.diff.trials, DEFAULT_TRIALS);
        assert_eq!(cli.diff.trials, 5);
    }

    #[test]
    fn differential_cli_accepts_batched_tasks_and_explicit_scheduler_limits() {
        let cli = TestCli::try_parse_from([
            "nanoeval",
            "--task",
            "tasks/first",
            "--task",
            "tasks/second",
            "--suite",
            "tasks/suite",
            "--codex-bin",
            "/opt/codex",
            "--trials",
            "7",
            "--concurrency",
            "12",
            "--max-memory-mb",
            "49152",
        ])
        .unwrap();

        assert_eq!(
            cli.diff.tasks,
            [
                Path::new("tasks/first").to_path_buf(),
                Path::new("tasks/second").to_path_buf()
            ]
        );
        assert_eq!(cli.diff.suites, [Path::new("tasks/suite").to_path_buf()]);
        assert_eq!(cli.diff.trials, 7);
        assert_eq!(cli.diff.concurrency, Some(12));
        assert_eq!(cli.diff.max_memory_mb, Some(49_152));
    }

    #[test]
    fn differential_cli_keeps_reanalysis_agent_free() {
        let cli =
            TestCli::try_parse_from(["nanoeval", "--reanalyze", "retained/comparison"]).unwrap();

        assert_eq!(
            cli.diff.reanalyze.as_deref(),
            Some(Path::new("retained/comparison"))
        );
        assert!(cli.diff.tasks.is_empty());
        assert!(cli.diff.codex_bin.is_none());
    }
}
