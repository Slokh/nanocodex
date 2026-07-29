#[cfg(not(target_family = "wasm"))]
mod agents_md;

#[cfg(not(target_family = "wasm"))]
#[path = "native.rs"]
mod platform;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
#[path = "web.rs"]
mod platform;

#[derive(Clone)]
pub(crate) struct LocalTimeContext {
    pub(crate) current_date: std::sync::Arc<str>,
    pub(crate) timezone: std::sync::Arc<str>,
}

pub(crate) use platform::ContextSource;
pub(super) use platform::ContextSourceConfig;
