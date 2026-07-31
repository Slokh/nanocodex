use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use hudsucker::hyper::{
    HeaderMap, Method,
    header::{Entry, HeaderName, HeaderValue},
    http::uri::Authority,
};
use reqwest::Url;
use thiserror::Error;

use crate::{EgressEnvironment, EgressLayer, EgressLayerError, EgressRequest};

const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_PLACEHOLDER_BYTES: usize = 4 * 1_024;
const MAX_PATH_PREFIX_BYTES: usize = 2_048;

/// Opaque reference understood only by a host-side [`SecretResolver`].
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SecretRef {
    provider: String,
    key: String,
}

impl SecretRef {
    /// Creates a provider-qualified reference without resolving its value.
    #[must_use]
    pub fn new(provider: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            key: key.into(),
        }
    }

    /// Returns the application-defined provider name.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the provider-owned opaque lookup key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// Host-side secret resolution failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SecretResolverError {
    /// The referenced value does not exist or is not authorized.
    #[error("secret is unavailable")]
    Unavailable,
}

/// Resolves credential values on the host for already-authorized requests.
///
/// Implementations should avoid caching unless their backing provider defines
/// an explicit rotation contract. Resolved values are never placed in child
/// configuration or returned by this library.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// Resolves one opaque reference.
    async fn resolve(&self, reference: &SecretRef) -> Result<String, SecretResolverError>;
}

#[async_trait]
impl<R> SecretResolver for Arc<R>
where
    R: SecretResolver + ?Sized,
{
    async fn resolve(&self, reference: &SecretRef) -> Result<String, SecretResolverError> {
        self.as_ref().resolve(reference).await
    }
}

/// Host-memory resolver for small applications and deterministic tests.
///
/// This type deliberately omits `Debug` so formatting it cannot expose the
/// values it owns. Use [`SecretResolver`] directly for external or rotating
/// secret stores.
#[derive(Clone, Default)]
pub struct StaticSecretResolver {
    values: BTreeMap<SecretRef, String>,
}

impl StaticSecretResolver {
    /// Creates an empty resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one host-only value.
    #[must_use]
    pub fn with_secret(mut self, reference: SecretRef, value: impl Into<String>) -> Self {
        self.values.insert(reference, value.into());
        self
    }
}

#[async_trait]
impl SecretResolver for StaticSecretResolver {
    async fn resolve(&self, reference: &SecretRef) -> Result<String, SecretResolverError> {
        self.values
            .get(reference)
            .cloned()
            .ok_or(SecretResolverError::Unavailable)
    }
}

/// One validated destination-bound header-replacement rule.
#[derive(Clone, Debug)]
pub struct SecretRule {
    id: String,
    source: SecretRef,
    upstream: Url,
    methods: Vec<Method>,
    path_prefixes: Vec<String>,
    header: HeaderName,
    placeholder: String,
    base_url_environment: String,
    placeholder_environment: String,
}

impl SecretRule {
    /// Starts a rule builder for one credential-free HTTPS or loopback HTTP upstream.
    #[must_use]
    pub fn builder(
        id: impl Into<String>,
        source: SecretRef,
        upstream: impl Into<String>,
    ) -> SecretRuleBuilder {
        SecretRuleBuilder {
            id: id.into(),
            source,
            upstream: upstream.into(),
            methods: Vec::new(),
            path_prefixes: Vec::new(),
            replacement: None,
            child_environment: None,
        }
    }

    /// Returns the stable non-secret rule identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the opaque host resolver reference.
    #[must_use]
    pub const fn source(&self) -> &SecretRef {
        &self.source
    }

    /// Returns the authorized upstream base URL.
    #[must_use]
    pub fn upstream(&self) -> &str {
        self.upstream.as_str().trim_end_matches('/')
    }
}

/// Builder for one [`SecretRule`].
pub struct SecretRuleBuilder {
    id: String,
    source: SecretRef,
    upstream: String,
    methods: Vec<Method>,
    path_prefixes: Vec<String>,
    replacement: Option<(String, String)>,
    child_environment: Option<(String, String)>,
}

impl SecretRuleBuilder {
    /// Adds one accepted HTTP method. No methods means all ordinary methods.
    #[must_use]
    pub fn method(mut self, method: Method) -> Self {
        if !self.methods.contains(&method) {
            self.methods.push(method);
        }
        self
    }

    /// Adds one accepted path-segment prefix, such as `/v1/responses`.
    #[must_use]
    pub fn path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefixes.push(prefix.into());
        self
    }

    /// Selects the header and public placeholder replaced at egress.
    #[must_use]
    pub fn replace_header(
        mut self,
        header: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> Self {
        self.replacement = Some((header.into(), placeholder.into()));
        self
    }

    /// Names the child variables receiving the base URL and public placeholder.
    #[must_use]
    pub fn child_environment(
        mut self,
        base_url: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> Self {
        self.child_environment = Some((base_url.into(), placeholder.into()));
        self
    }

    /// Validates and creates the destination-bound rule.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe origins, paths, headers, placeholders, or
    /// child environment names.
    pub fn build(self) -> Result<SecretRule, SecretConfigError> {
        validate_identifier(&self.id).ok_or(SecretConfigError::InvalidId)?;
        if self.source.provider.trim().is_empty()
            || self.source.key.trim().is_empty()
            || self.source.provider.len() > MAX_IDENTIFIER_BYTES
            || self.source.key.len() > 4 * 1_024
        {
            return Err(SecretConfigError::InvalidSource);
        }
        let upstream =
            Url::parse(&self.upstream).map_err(|_| SecretConfigError::InvalidUpstream)?;
        if !matches!(upstream.scheme(), "http" | "https")
            || upstream.host_str().is_none()
            || !upstream.username().is_empty()
            || upstream.password().is_some()
            || upstream.query().is_some()
            || upstream.fragment().is_some()
            || !valid_path_prefix(upstream.path())
        {
            return Err(SecretConfigError::InvalidUpstream);
        }
        if upstream.scheme() == "http" && !upstream.host_str().is_some_and(is_loopback_host) {
            return Err(SecretConfigError::InsecureUpstream);
        }
        if self.methods.iter().any(|method| {
            !matches!(
                *method,
                Method::GET
                    | Method::POST
                    | Method::PUT
                    | Method::PATCH
                    | Method::DELETE
                    | Method::HEAD
                    | Method::OPTIONS
            )
        }) {
            return Err(SecretConfigError::InvalidMethod);
        }
        if self
            .path_prefixes
            .iter()
            .any(|prefix| !valid_path_prefix(prefix))
        {
            return Err(SecretConfigError::InvalidPathPrefix);
        }
        let (header, placeholder) = self
            .replacement
            .ok_or(SecretConfigError::MissingReplacement)?;
        let header = HeaderName::from_bytes(header.as_bytes())
            .map_err(|_| SecretConfigError::InvalidHeader)?;
        if is_transport_owned_header(&header) {
            return Err(SecretConfigError::InvalidHeader);
        }
        if placeholder.is_empty()
            || placeholder.len() > MAX_PLACEHOLDER_BYTES
            || HeaderValue::from_str(&placeholder).is_err()
        {
            return Err(SecretConfigError::InvalidPlaceholder);
        }
        let (base_url_environment, placeholder_environment) = self
            .child_environment
            .ok_or(SecretConfigError::MissingChildEnvironment)?;
        if !valid_environment_name(&base_url_environment)
            || !valid_environment_name(&placeholder_environment)
            || base_url_environment == placeholder_environment
        {
            return Err(SecretConfigError::InvalidEnvironment);
        }
        Ok(SecretRule {
            id: self.id,
            source: self.source,
            upstream,
            methods: self.methods,
            path_prefixes: self.path_prefixes,
            header,
            placeholder,
            base_url_environment,
            placeholder_environment,
        })
    }
}

/// Policy for destinations not claimed by a secret rule.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnmatchedEgress {
    /// Reject unmatched destinations before contacting them.
    #[default]
    Deny,
    /// Forward unmatched destinations to later egress layers or the origin.
    Allow,
}

/// Host-side secret replacement as one independently composable egress layer.
#[derive(Clone)]
pub struct SecretEgress {
    resolver: Arc<dyn SecretResolver>,
    rules: Arc<[SecretRule]>,
    environment: EgressEnvironment,
    unmatched: UnmatchedEgress,
}

impl SecretEgress {
    /// Starts a fail-closed secret layer builder with one host-side resolver.
    #[must_use]
    pub fn builder<R>(resolver: R) -> SecretEgressBuilder
    where
        R: SecretResolver + 'static,
    {
        SecretEgressBuilder {
            resolver: Arc::new(resolver),
            rules: Vec::new(),
            unmatched: UnmatchedEgress::Deny,
        }
    }

    fn from_parts(
        resolver: Arc<dyn SecretResolver>,
        rules: Vec<SecretRule>,
        unmatched: UnmatchedEgress,
    ) -> Result<Self, SecretConfigError> {
        if rules.is_empty() {
            return Err(SecretConfigError::MissingRules);
        }
        let mut ids = BTreeMap::new();
        let mut environment = BTreeMap::new();
        for rule in &rules {
            if ids.insert(rule.id.clone(), ()).is_some() {
                return Err(SecretConfigError::DuplicateId(rule.id.clone()));
            }
            insert_environment(
                &mut environment,
                &rule.base_url_environment,
                rule.upstream().to_owned(),
            )?;
            insert_environment(
                &mut environment,
                &rule.placeholder_environment,
                rule.placeholder.clone(),
            )?;
        }
        Ok(Self {
            resolver,
            rules: rules.into(),
            environment: EgressEnvironment::new(
                environment
                    .into_iter()
                    .map(|(name, value)| (name.into(), value.into())),
            ),
            unmatched,
        })
    }

    /// Returns child variables containing only public origins and placeholders.
    #[must_use]
    pub const fn environment(&self) -> &EgressEnvironment {
        &self.environment
    }
}

/// Builder for one host-owned secret replacement layer.
pub struct SecretEgressBuilder {
    resolver: Arc<dyn SecretResolver>,
    rules: Vec<SecretRule>,
    unmatched: UnmatchedEgress,
}

impl SecretEgressBuilder {
    /// Appends one validated destination-bound rule.
    #[must_use]
    pub fn rule(mut self, rule: SecretRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Appends validated destination-bound rules in declaration order.
    #[must_use]
    pub fn rules(mut self, rules: impl IntoIterator<Item = SecretRule>) -> Self {
        self.rules.extend(rules);
        self
    }

    /// Selects policy for destinations not claimed by a secret rule.
    #[must_use]
    pub const fn unmatched(mut self, unmatched: UnmatchedEgress) -> Self {
        self.unmatched = unmatched;
        self
    }

    /// Validates unique IDs and child variables and creates the layer.
    ///
    /// # Errors
    ///
    /// Returns an error when there are no rules or when two rules claim the
    /// same ID or environment name.
    pub fn build(self) -> Result<SecretEgress, SecretConfigError> {
        SecretEgress::from_parts(self.resolver, self.rules, self.unmatched)
    }
}

impl SecretEgress {
    async fn authorize_http_request(
        &self,
        request: &mut reqwest::Request,
    ) -> Result<(), EgressLayerError> {
        let rule = match select_rule(&self.rules, request)? {
            RuleSelection::Replace(rule) => rule,
            RuleSelection::Unmatched if self.unmatched == UnmatchedEgress::Allow => {
                return Ok(());
            }
            RuleSelection::Unmatched => return Err(EgressLayerError::Denied),
        };
        let secret = self
            .resolver
            .resolve(&rule.source)
            .await
            .map_err(|_| EgressLayerError::Unavailable)?;
        replace_header(request.headers_mut(), rule, &secret)?;
        tracing::info!(
            target: "nanocodex_egress",
            content_kind = "egress.secret.request.headers",
            content = ?request.headers(),
            "trace content"
        );
        tracing::info!(
            target: "nanocodex_egress",
            secret_rule_id = %rule.id,
            http.request.method = %request.method(),
            "authorized host-side secret replacement"
        );
        Ok(())
    }
}

#[async_trait]
impl EgressLayer for SecretEgress {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        extensions: &mut ::http::Extensions,
        next: reqwest_middleware::Next<'_>,
    ) -> reqwest_middleware::Result<reqwest::Response> {
        self.authorize_http_request(&mut request)
            .await
            .map_err(reqwest_middleware::Error::middleware)?;
        next.run(request, extensions).await
    }

    async fn authorize_connect(&self, request: &EgressRequest) -> Result<(), EgressLayerError> {
        authorize_connect_request(request, &self.rules, self.unmatched)
    }

    fn environment(&self) -> EgressEnvironment {
        self.environment.clone()
    }
}

fn authorize_connect_request(
    request: &EgressRequest,
    rules: &[SecretRule],
    unmatched: UnmatchedEgress,
) -> Result<(), EgressLayerError> {
    if unmatched == UnmatchedEgress::Allow {
        return Ok(());
    }
    let authority = request
        .uri()
        .authority()
        .cloned()
        .or_else(|| request.uri().to_string().parse().ok())
        .ok_or(EgressLayerError::InvalidRequest)?;
    rules
        .iter()
        .any(|rule| matching_upstream(rule, "https", &authority))
        .then_some(())
        .ok_or(EgressLayerError::Denied)
}

enum RuleSelection<'a> {
    Replace(&'a SecretRule),
    Unmatched,
}

fn select_rule<'a>(
    rules: &'a [SecretRule],
    request: &reqwest::Request,
) -> Result<RuleSelection<'a>, EgressLayerError> {
    let path = safe_request_path(request.url().path()).ok_or(EgressLayerError::InvalidRequest)?;
    let mut origin_matched = false;
    let mut selected = None;
    for rule in rules {
        if !matching_upstream_url(rule, request.url()) {
            continue;
        }
        origin_matched = true;
        if !allows_request(rule, request.method(), &path)
            || !has_single_placeholder_header(request.headers(), rule)
        {
            continue;
        }
        if selected.replace(rule).is_some() {
            return Err(EgressLayerError::Denied);
        }
    }
    match selected {
        Some(rule) => Ok(RuleSelection::Replace(rule)),
        None if origin_matched => Err(EgressLayerError::Denied),
        None => Ok(RuleSelection::Unmatched),
    }
}

fn matching_upstream(rule: &SecretRule, scheme: &str, authority: &Authority) -> bool {
    rule.upstream.scheme() == scheme
        && rule
            .upstream
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(authority.host()))
        && rule.upstream.port_or_known_default()
            == authority
                .port_u16()
                .or_else(|| (scheme == "https").then_some(443))
                .or_else(|| (scheme == "http").then_some(80))
}

fn matching_upstream_url(rule: &SecretRule, url: &Url) -> bool {
    rule.upstream.scheme() == url.scheme()
        && rule
            .upstream
            .host_str()
            .zip(url.host_str())
            .is_some_and(|(expected, actual)| expected.eq_ignore_ascii_case(actual))
        && rule.upstream.port_or_known_default() == url.port_or_known_default()
}

fn allows_request(rule: &SecretRule, method: &Method, path: &str) -> bool {
    let supported = matches!(
        *method,
        Method::GET
            | Method::POST
            | Method::PUT
            | Method::PATCH
            | Method::DELETE
            | Method::HEAD
            | Method::OPTIONS
    );
    let within_upstream = safe_request_path(rule.upstream.path())
        .is_some_and(|prefix| path_prefix_matches(&prefix, path));
    supported
        && within_upstream
        && (rule.methods.is_empty() || rule.methods.contains(method))
        && (rule.path_prefixes.is_empty()
            || rule
                .path_prefixes
                .iter()
                .any(|prefix| path_prefix_matches(prefix, path)))
}

fn has_single_placeholder_header(headers: &HeaderMap, rule: &SecretRule) -> bool {
    let mut values = headers.get_all(&rule.header).iter();
    let Some(value) = values.next() else {
        return false;
    };
    values.next().is_none()
        && value
            .to_str()
            .is_ok_and(|value| value.matches(&rule.placeholder).count() == 1)
}

fn replace_header(
    headers: &mut HeaderMap,
    rule: &SecretRule,
    secret: &str,
) -> Result<(), EgressLayerError> {
    let Entry::Occupied(mut entry) = headers.entry(rule.header.clone()) else {
        return Err(EgressLayerError::Denied);
    };
    for value in entry.iter_mut() {
        let current = value.to_str().map_err(|_| EgressLayerError::Denied)?;
        if current.contains(&rule.placeholder) {
            let replacement = current.replace(&rule.placeholder, secret);
            *value =
                HeaderValue::from_str(&replacement).map_err(|_| EgressLayerError::Unavailable)?;
            return Ok(());
        }
    }
    Err(EgressLayerError::Denied)
}

fn path_prefix_matches(prefix: &str, path: &str) -> bool {
    prefix == "/"
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| prefix.ends_with('/') || suffix.starts_with('/'))
}

fn valid_path_prefix(prefix: &str) -> bool {
    prefix.starts_with('/')
        && prefix.len() <= MAX_PATH_PREFIX_BYTES
        && safe_request_path(prefix).is_some()
}

fn safe_request_path(path: &str) -> Option<Cow<'_, str>> {
    if path.contains('\\')
        || path.split('/').any(|segment| segment == "..")
        || contains_ambiguous_path_escape(path)
    {
        return None;
    }
    if path.starts_with('/') && !path.starts_with("//") {
        Some(Cow::Borrowed(path))
    } else {
        Some(Cow::Owned(format!("/{}", path.trim_start_matches('/'))))
    }
}

fn contains_ambiguous_path_escape(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        let encoded = bytes
            .get(index + 1..index + 3)
            .and_then(|digits| decode_hex_byte(digits[0], digits[1]));
        let Some(encoded) = encoded else {
            return true;
        };
        if matches!(encoded, b'%' | b'.' | b'/' | b'\\' | b'?' | b'#') {
            return true;
        }
        index += 3;
    }
    false
}

fn decode_hex_byte(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? << 4 | hex_value(low)?)
}

const fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

fn validate_identifier(value: &str) -> Option<()> {
    (!value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
    .then_some(())
}

fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_uppercase())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_transport_owned_header(header: &HeaderName) -> bool {
    [
        "host",
        "content-length",
        "transfer-encoding",
        "connection",
        "proxy-authorization",
    ]
    .iter()
    .any(|reserved| header.as_str().eq_ignore_ascii_case(reserved))
}

fn insert_environment(
    environment: &mut BTreeMap<String, String>,
    name: &str,
    value: String,
) -> Result<(), SecretConfigError> {
    if environment.insert(name.to_owned(), value).is_some() {
        return Err(SecretConfigError::DuplicateEnvironment(name.to_owned()));
    }
    Ok(())
}

/// Invalid secret-rule configuration.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SecretConfigError {
    /// A secret layer was configured without any destination-bound rules.
    #[error("secret egress requires at least one rule")]
    MissingRules,
    /// A rule ID was empty, unbounded, or contained unsupported bytes.
    #[error("secret rule id must be a bounded ASCII identifier")]
    InvalidId,
    /// A resolver provider or key was empty or unbounded.
    #[error("secret source must contain a bounded provider and key")]
    InvalidSource,
    /// An upstream was not a credential-free absolute HTTP(S) base URL.
    #[error("secret upstream must be a credential-free absolute HTTP(S) base URL")]
    InvalidUpstream,
    /// A remote plaintext upstream could expose the resolved credential.
    #[error("secret upstream must use HTTPS unless it is loopback")]
    InsecureUpstream,
    /// A CONNECT or extension method was configured for replacement.
    #[error("secret rules support ordinary HTTP methods only")]
    InvalidMethod,
    /// A path prefix was relative or ambiguously encoded.
    #[error("secret path prefixes must be safe bounded absolute paths")]
    InvalidPathPrefix,
    /// No replacement header and placeholder were configured.
    #[error("secret rule requires a replacement header and placeholder")]
    MissingReplacement,
    /// A header name was invalid or transport-owned.
    #[error("secret replacement header is invalid or transport-owned")]
    InvalidHeader,
    /// A placeholder was empty, unbounded, or not a valid header value.
    #[error("secret placeholder must be a non-empty bounded HTTP header value")]
    InvalidPlaceholder,
    /// Child base URL and placeholder variables were not configured.
    #[error("secret rule requires child base URL and placeholder variables")]
    MissingChildEnvironment,
    /// A child variable was invalid or both values used the same name.
    #[error("secret child variables must be distinct uppercase shell identifiers")]
    InvalidEnvironment,
    /// Two rules used the same stable ID.
    #[error("duplicate secret rule id `{0}`")]
    DuplicateId(String),
    /// Two rules claimed the same child variable.
    #[error("duplicate secret child environment `{0}`")]
    DuplicateEnvironment(String),
}

#[cfg(test)]
mod tests {
    use hudsucker::hyper::Uri;

    use super::*;

    #[test]
    fn secret_layer_rejects_an_empty_rule_set() {
        let result = SecretEgress::builder(StaticSecretResolver::new()).build();
        assert!(matches!(result, Err(SecretConfigError::MissingRules)));
    }

    #[test]
    fn replacement_rule_exports_only_public_child_values() {
        let rule = SecretRule::builder(
            "openai",
            SecretRef::new("environment", "OPENAI_API_KEY"),
            "https://api.openai.com",
        )
        .method(Method::POST)
        .path_prefix("/v1/responses")
        .replace_header("authorization", "nanocodex-secret-openai")
        .child_environment("OPENAI_BASE_URL", "OPENAI_API_KEY")
        .build()
        .unwrap();
        struct Unavailable;
        #[async_trait]
        impl SecretResolver for Unavailable {
            async fn resolve(&self, _reference: &SecretRef) -> Result<String, SecretResolverError> {
                Err(SecretResolverError::Unavailable)
            }
        }
        let rules = SecretEgress::builder(Unavailable)
            .rule(rule)
            .build()
            .unwrap();
        assert_eq!(
            rules.environment().get("OPENAI_BASE_URL"),
            Some(std::ffi::OsStr::new("https://api.openai.com"))
        );
        assert_eq!(
            rules.environment().get("OPENAI_API_KEY"),
            Some(std::ffi::OsStr::new("nanocodex-secret-openai"))
        );
    }

    #[test]
    fn rules_reject_ambiguous_paths_and_placeholders() {
        let build = |path: &str, placeholder: &str| {
            SecretRule::builder(
                "openai",
                SecretRef::new("environment", "OPENAI_API_KEY"),
                "https://api.openai.com",
            )
            .path_prefix(path)
            .replace_header("authorization", placeholder)
            .child_environment("OPENAI_BASE_URL", "OPENAI_API_KEY")
            .build()
        };
        assert_eq!(
            build("/v1/%2e%2e/admin", "placeholder").unwrap_err(),
            SecretConfigError::InvalidPathPrefix
        );
        assert_eq!(
            build("/v1/responses", "").unwrap_err(),
            SecretConfigError::InvalidPlaceholder
        );

        let insecure = SecretRule::builder(
            "openai",
            SecretRef::new("environment", "OPENAI_API_KEY"),
            "http://api.openai.com",
        )
        .replace_header("authorization", "placeholder")
        .child_environment("OPENAI_BASE_URL", "OPENAI_API_KEY")
        .build();
        assert_eq!(insecure.unwrap_err(), SecretConfigError::InsecureUpstream);
    }

    #[tokio::test]
    async fn duplicate_replacement_headers_fail_closed() {
        let reference = SecretRef::new("memory", "openai");
        let rule = SecretRule::builder("openai", reference.clone(), "https://api.openai.com")
            .method(Method::POST)
            .path_prefix("/v1/responses")
            .replace_header("authorization", "nanocodex-secret-openai")
            .child_environment("OPENAI_BASE_URL", "OPENAI_API_KEY")
            .build()
            .unwrap();
        let layer =
            SecretEgress::builder(StaticSecretResolver::new().with_secret(reference, "host-only"))
                .rule(rule)
                .build()
                .unwrap();
        let mut request = reqwest::Request::new(
            Method::POST,
            Url::parse("https://api.openai.com/v1/responses").unwrap(),
        );
        request.headers_mut().append(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer nanocodex-secret-openai"),
        );
        request.headers_mut().append(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer attacker-controlled"),
        );

        assert_eq!(
            layer.authorize_http_request(&mut request).await,
            Err(EgressLayerError::Denied)
        );
    }

    #[tokio::test]
    async fn connect_policy_only_opens_configured_tls_origins() {
        let rule = SecretRule::builder(
            "openai",
            SecretRef::new("environment", "OPENAI_API_KEY"),
            "https://api.openai.com",
        )
        .replace_header("authorization", "nanocodex-secret-openai")
        .child_environment("OPENAI_BASE_URL", "OPENAI_API_KEY")
        .build()
        .unwrap();
        let layer = SecretEgress::builder(StaticSecretResolver::new())
            .rule(rule)
            .build()
            .unwrap();
        let request = |authority: &'static str| {
            EgressRequest::new(
                Method::CONNECT,
                Uri::from_static(authority),
                HeaderMap::new(),
            )
        };

        assert!(
            layer
                .authorize_connect(&request("api.openai.com:443"))
                .await
                .is_ok()
        );
        assert_eq!(
            layer
                .authorize_connect(&request("attacker.invalid:443"))
                .await,
            Err(EgressLayerError::Denied)
        );
    }
}
