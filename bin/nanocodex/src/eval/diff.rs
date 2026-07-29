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

#[derive(Debug, Default, Eq, PartialEq)]
struct DifferentialScoreSummary {
    attempts: usize,
    valid: usize,
    infrastructure: usize,
    incomplete: usize,
    nanocodex_passes: usize,
    codex_passes: usize,
}

impl DifferentialScoreSummary {
    const fn observe(
        &mut self,
        classification: DifferentialClassification,
        infrastructure: bool,
        operational_error: bool,
    ) {
        self.attempts += 1;
        if infrastructure {
            self.infrastructure += 1;
            return;
        }
        if operational_error {
            self.incomplete += 1;
            return;
        }
        match classification {
            DifferentialClassification::BothPassed => {
                self.valid += 1;
                self.nanocodex_passes += 1;
                self.codex_passes += 1;
            }
            DifferentialClassification::CodexOnlyPassed => {
                self.valid += 1;
                self.codex_passes += 1;
            }
            DifferentialClassification::NanocodexOnlyPassed => {
                self.valid += 1;
                self.nanocodex_passes += 1;
            }
            DifferentialClassification::NeitherPassed => {
                self.valid += 1;
            }
            DifferentialClassification::Incomplete => {
                self.incomplete += 1;
            }
        }
    }
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

    /// Hard ceiling on task-declared memory across live arms in this process.
    ///
    /// Both arms are charged at pair start. A task whose pair exceeds this
    /// ceiling is rejected instead of being admitted as an oversized job.
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
        let requested_trials = usize::from(self.trials);
        let task_names = tasks
            .iter()
            .map(|task| task.name().to_owned())
            .collect::<Vec<_>>();
        let (automatic_concurrency, automatic_memory_mb) =
            run::automatic_scheduling_defaults(self.host_utilization);
        let concurrency = self.concurrency.unwrap_or(automatic_concurrency);
        let max_memory_mb = self.max_memory_mb.or(automatic_memory_mb);
        if let Some(max_memory_mb) = max_memory_mb {
            for task in &tasks {
                validate_pair_memory_limit(task.name(), task.resources().memory_mb, max_memory_mb)?;
            }
        }
        eprintln!(
            "Differential sweep: {} task(s) × k={} · up to {} pair(s) · {}",
            tasks.len(),
            self.trials,
            concurrency,
            max_memory_mb.map_or_else(
                || "unbounded declared memory".to_owned(),
                |memory| format!("{memory} MiB live-arm memory ceiling")
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
        let output = self.output;
        let mut evaluator = DifferentialEvaluator::builder(nanocodex)
            .codex(
                self.codex_bin
                    .ok_or_else(|| eyre!("--codex-bin is required unless --reanalyze is used"))?,
                codex_auth,
            )
            .vm(vm)
            .output_directory(&output)
            .thinking(thinking)
            .web_search(web_search)
            .codex_tool_mode(self.codex_tool_mode.into())
            .nanocodex_executable(
                ExecutableIdentity::new(current_executable, env!("NANOCODEX_SEMVER_VERSION"))
                    .git_sha(env!("VERGEN_GIT_SHA"))
                    .built_at(env!("VERGEN_BUILD_TIMESTAMP")),
            )
            .max_concurrency(usize::from(concurrency))
            .max_infrastructure_replacements(requested_trials);
        if let Some(max_memory_mb) = max_memory_mb {
            evaluator = evaluator.max_memory_mb(max_memory_mb);
        }
        let evaluator = evaluator.build()?;
        let comparison_count = tasks.len().saturating_mul(requested_trials);
        let interrupts = run::ctrl_c_interrupt()?;
        let execution = run::finish_or_drain(
            evaluator.tasks_n(tasks, requested_trials),
            interrupts,
            comparison_count,
            || {
                let admitted = evaluator.begin_drain();
                eprintln!(
                    "Interrupt received; stopped admitting new comparisons after {admitted} \
                     pair(s), draining admitted work; press Ctrl-C again to abort"
                );
                admitted
            },
        )
        .await?;
        let run::DrainExecution {
            result,
            interrupted,
            interrupt,
            ..
        } = execution;
        run::finish_or_interrupt(
            async move {
                let reports = match result {
                    Ok(reports) => reports,
                    Err(error) if interrupted => {
                        return Err(eyre!(
                            "differential sweep interrupted after draining admitted comparisons; \
                             queued comparisons were not started and retained evidence remains \
                             under {} ({error})",
                            output.display()
                        ));
                    }
                    Err(error) => return Err(error.into()),
                };
                write_score_summaries(&task_names, requested_trials, &reports);
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
                if interrupted {
                    return Err(eyre!(
                        "differential sweep interrupted after draining admitted comparisons; \
                         queued comparisons were not started and retained evidence remains under \
                         {}",
                        output.display()
                    ));
                }
                if let Some((task_name, valid_pairs)) = task_names.iter().find_map(|task_name| {
                    let valid_pairs = reports
                        .iter()
                        .filter(|report| {
                            report.task_name() == task_name && !report.has_infrastructure_failure()
                        })
                        .count();
                    (valid_pairs < requested_trials).then_some((task_name, valid_pairs))
                }) {
                    let evidence = reports
                        .iter()
                        .find(|report| {
                            report.task_name() == task_name && report.has_infrastructure_failure()
                        })
                        .map_or(output.as_path(), DifferentialReport::comparison_path);
                    return Err(eyre!(
                        "task {task_name} retained {valid_pairs}/{requested_trials} valid matched \
                         pairs after {requested_trials} infrastructure replacement(s); evidence \
                         retained at {}",
                        evidence.display()
                    ));
                }
                Ok(())
            },
            interrupt,
        )
        .await??;
        Ok(())
    }
}

fn write_score_summaries(
    task_names: &[String],
    requested_trials: usize,
    reports: &[DifferentialReport],
) {
    for task_name in task_names {
        let mut summary = DifferentialScoreSummary::default();
        for report in reports
            .iter()
            .filter(|report| report.task_name() == task_name)
        {
            summary.observe(
                report.classification(),
                report.has_infrastructure_failure(),
                report.has_operational_error(),
            );
        }
        eprintln!(
            "Differential score: {task_name} · valid {}/{} · attempts {} · infrastructure {} · \
             incomplete {} · Nanocodex {}/{} · stock Codex {}/{}",
            summary.valid,
            requested_trials,
            summary.attempts,
            summary.infrastructure,
            summary.incomplete,
            summary.nanocodex_passes,
            summary.valid,
            summary.codex_passes,
            summary.valid
        );
    }
}

fn validate_pair_memory_limit(
    task_name: &str,
    arm_memory_mb: u64,
    max_memory_mb: u64,
) -> Result<()> {
    let pair_memory_mb = arm_memory_mb.checked_mul(2).ok_or_else(|| {
        eyre!("task {task_name} pair memory overflows while applying --max-memory-mb")
    })?;
    if pair_memory_mb > max_memory_mb {
        return Err(eyre!(
            "task {task_name} requires {pair_memory_mb} MiB for its two arms, exceeding the \
             {max_memory_mb} MiB --max-memory-mb ceiling; raise the ceiling or schedule this task \
             in a separate process"
        ));
    }
    Ok(())
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
    use nanocodex_eval::DifferentialClassification;

    use super::{Diff, DifferentialScoreSummary, validate_pair_memory_limit};
    use crate::eval::run::DEFAULT_TRIALS;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        diff: Diff,
    }

    #[test]
    fn differential_memory_ceiling_is_strict_for_a_pair() {
        validate_pair_memory_limit("terminal-bench/small", 2_048, 4_096).unwrap();

        let error = validate_pair_memory_limit("terminal-bench/large", 8_192, 12_288).unwrap_err();
        assert_eq!(
            error.to_string(),
            "task terminal-bench/large requires 16384 MiB for its two arms, exceeding the 12288 \
             MiB --max-memory-mb ceiling; raise the ceiling or schedule this task in a separate \
             process"
        );
    }

    #[test]
    fn differential_score_summary_excludes_infrastructure_and_incomplete_pairs() {
        let mut summary = DifferentialScoreSummary::default();
        summary.observe(DifferentialClassification::BothPassed, false, false);
        summary.observe(DifferentialClassification::CodexOnlyPassed, false, false);
        summary.observe(
            DifferentialClassification::NanocodexOnlyPassed,
            false,
            false,
        );
        summary.observe(DifferentialClassification::NeitherPassed, false, false);
        summary.observe(DifferentialClassification::BothPassed, true, false);
        summary.observe(DifferentialClassification::BothPassed, false, true);
        summary.observe(DifferentialClassification::Incomplete, false, false);

        assert_eq!(
            summary,
            DifferentialScoreSummary {
                attempts: 7,
                valid: 4,
                infrastructure: 1,
                incomplete: 2,
                nanocodex_passes: 2,
                codex_passes: 2,
            }
        );
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
