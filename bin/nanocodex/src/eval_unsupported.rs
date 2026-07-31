use clap::Args;
use eyre::{Result, eyre};

#[derive(Args)]
pub(crate) struct Eval {}

impl Eval {
    pub(crate) const fn requires_synchronous_vm(&self) -> bool {
        false
    }

    pub(crate) fn run_synchronous_vm(&self) -> Result<()> {
        Err(unsupported_target())
    }

    pub(crate) async fn run(self) -> Result<()> {
        Err(unsupported_target())
    }
}

fn unsupported_target() -> eyre::Report {
    eyre!("VM evaluation requires glibc Linux or Apple Silicon macOS")
}
