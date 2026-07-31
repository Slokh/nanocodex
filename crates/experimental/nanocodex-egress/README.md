# nanocodex-egress

`nanocodex-egress` is Nanocodex's unpublished, experimental HTTP egress
transport. It owns the authenticated loopback proxy shared by application
protocol layers and host-side secret replacement.

The crate owns proxy authentication, TLS interception, bounded replayable
request bodies, forwarding concurrency, and lifecycle. Application protocols
remain outside the crate and compose through `EgressLayer`; the Nanocodex
binary implements its Tempo payment layer using MPP's request middleware.
The bundled `SecretEgress` layer keeps credential values in the host process
and exposes only a public upstream URL and placeholder to a child.

```rust,no_run
use nanocodex_egress::{
    EgressProxy, SecretEgress, SecretRef, SecretRule, StaticSecretResolver,
};
use nanocodex_egress::http::Method;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let reference = SecretRef::new("memory", "service-token");
let resolver = StaticSecretResolver::new()
    .with_secret(reference.clone(), "host-only-value");
let rule = SecretRule::builder("service", reference, "https://service.example")
    .method(Method::POST)
    .path_prefix("/v1/execute")
    .replace_header("authorization", "nanocodex-secret-service")
    .child_environment("SERVICE_BASE_URL", "SERVICE_API_KEY")
    .build()?;
let secrets = SecretEgress::builder(resolver).rule(rule).build()?;
let proxy = EgressProxy::builder()
    .layer(secrets)
    .spawn()
    .await?;

let child_environment = proxy.environment();
assert!(!child_environment.is_empty());
assert!(child_environment.iter().all(|(_, value)| value != "host-only-value"));
assert!(proxy.ca_certificate_path().is_file());
proxy.shutdown().await?;
# Ok(())
# }
```

Layers run in builder order. Apply `environment()` only to tool child
processes, not to Nanocodex's control-plane process. It contains the proxy's
short-lived authentication capability, the ephemeral CA path, and each secret
rule's public base URL and placeholder; it never contains a resolved secret.

A secret rule binds replacement to an exact origin plus optional methods and
path-segment prefixes. Requests to a claimed origin fail closed when the method,
path, or placeholder does not match. Remote upstreams must use HTTPS; plaintext
HTTP is accepted only for loopback development services. The resolver runs only
after policy accepts a request, allowing application-defined rotation without
putting secret-store access in child code.

`SecretEgress` denies destinations not claimed by a rule by default. Set
`UnmatchedEgress::Allow` only when another layer or the origin should receive
unclaimed traffic. CONNECT tunnels are checked before an origin connection is
opened, and layer-provided child variables cannot override proxy transport
variables.
