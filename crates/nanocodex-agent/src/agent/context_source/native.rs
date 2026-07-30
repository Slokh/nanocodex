use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use super::agents_md::{combine_instructions, load_global_instructions, load_instructions};
use crate::{NanocodexError, Result};

#[derive(Clone, Default)]
pub(crate) struct ContextSourceConfig {
    codex_home: Option<PathBuf>,
    local_time_context: Option<super::LocalTimeContext>,
    project_instructions: ProjectInstructionsSource,
}

#[derive(Clone, Default)]
enum ProjectInstructionsSource {
    #[default]
    Native,
    Snapshot(Option<Arc<str>>),
}

impl ContextSourceConfig {
    pub(crate) fn set_codex_home(&mut self, codex_home: PathBuf) {
        self.codex_home = Some(codex_home);
    }

    pub(crate) fn codex_home(&self) -> Option<&Path> {
        self.codex_home.as_deref()
    }

    pub(crate) fn set_local_time_context(&mut self, context: super::LocalTimeContext) {
        self.local_time_context = Some(context);
    }

    pub(crate) fn set_project_instructions_snapshot(&mut self, instructions: Option<Arc<str>>) {
        self.project_instructions = ProjectInstructionsSource::Snapshot(instructions);
    }

    pub(crate) const fn local_time_context(&self) -> Option<&super::LocalTimeContext> {
        self.local_time_context.as_ref()
    }

    pub(crate) fn build(&self) -> ContextSource {
        ContextSource {
            global_instructions: load_global_instructions(self.codex_home()),
            local_time_context: self.local_time_context.clone(),
            project_instructions: self.project_instructions.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ContextSource {
    global_instructions: Option<Arc<str>>,
    local_time_context: Option<super::LocalTimeContext>,
    project_instructions: ProjectInstructionsSource,
}

impl ContextSource {
    pub(crate) fn resolve_workspace(&self, requested: Option<&str>) -> Result<String> {
        let requested = PathBuf::from(requested.unwrap_or("."));
        let resolved = std::fs::canonicalize(&requested).map_err(|source| {
            NanocodexError::ResolveWorkspace {
                path: requested,
                source,
            }
        })?;
        if !resolved.is_dir() {
            return Err(NanocodexError::WorkspaceNotDirectory { path: resolved });
        }
        resolved
            .into_os_string()
            .into_string()
            .map_err(|path| NanocodexError::WorkspaceNotUtf8 {
                path: PathBuf::from(path),
            })
    }

    pub(crate) fn project_instructions(&self, workspace: &str) -> Option<String> {
        match &self.project_instructions {
            ProjectInstructionsSource::Snapshot(project) => {
                combine_instructions(self.global_instructions.as_deref(), project.as_deref())
            }
            ProjectInstructionsSource::Native => {
                load_instructions(Path::new(workspace), self.global_instructions.as_deref())
            }
        }
    }

    pub(crate) fn global_instructions(&self) -> Option<Arc<str>> {
        self.global_instructions.as_ref().map(Arc::clone)
    }

    pub(crate) const fn local_time_context(&self) -> Option<&super::LocalTimeContext> {
        self.local_time_context.as_ref()
    }

    pub(crate) fn with_fallback_global(mut self, fallback: Option<Arc<str>>) -> Self {
        if self.global_instructions.is_none() {
            self.global_instructions = fallback;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_project_snapshot_replaces_native_workspace_discovery() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("AGENTS.md"), "host instructions").unwrap();
        let mut config = ContextSourceConfig::default();
        config.set_project_instructions_snapshot(Some(Arc::from("guest instructions")));

        assert_eq!(
            config
                .build()
                .project_instructions(workspace.path().to_str().unwrap())
                .as_deref(),
            Some("guest instructions")
        );
    }
}
