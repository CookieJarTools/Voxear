use tracing_subscriber::EnvFilter;

/// Installs the global log subscriber. `VOXEAR_LOG` (or `RUST_LOG`) overrides
/// the default `info` filter, e.g. `VOXEAR_LOG=voxear_rhi=debug`.
pub fn init() {
    let filter = std::env::var("VOXEAR_LOG").or_else(|_| std::env::var("RUST_LOG")).unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::new(filter)).with_target(false).try_init();
}
