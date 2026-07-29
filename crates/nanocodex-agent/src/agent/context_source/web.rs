use std::sync::Arc;

use crate::Result;

#[derive(Clone, Default)]
pub(crate) struct ContextSourceConfig {
    local_time_context: Option<super::LocalTimeContext>,
}

impl ContextSourceConfig {
    pub(crate) fn set_local_time_context(&mut self, context: super::LocalTimeContext) {
        self.local_time_context = Some(context);
    }

    pub(crate) const fn local_time_context(&self) -> Option<&super::LocalTimeContext> {
        self.local_time_context.as_ref()
    }

    pub(crate) fn build(&self) -> ContextSource {
        ContextSource {
            local_time_context: self.local_time_context.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ContextSource {
    local_time_context: Option<super::LocalTimeContext>,
}

impl ContextSource {
    pub(crate) fn resolve_workspace(&self, requested: Option<&str>) -> Result<String> {
        Ok(requested.unwrap_or(".").to_owned())
    }

    pub(crate) const fn project_instructions(&self, _workspace: &str) -> Option<String> {
        None
    }

    pub(crate) const fn global_instructions(&self) -> Option<Arc<str>> {
        None
    }

    pub(crate) const fn local_time_context(&self) -> Option<&super::LocalTimeContext> {
        self.local_time_context.as_ref()
    }

    pub(crate) fn with_fallback_global(self, _fallback: Option<Arc<str>>) -> Self {
        self
    }
}
