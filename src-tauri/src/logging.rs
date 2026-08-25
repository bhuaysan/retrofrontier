use tracing_subscriber::EnvFilter;

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("retrofrontier=info,tauri=warn,wry=warn,tao=warn"));

    // M1 writes structured development logs to stdout/stderr. A file layer can be
    // added later once the application-data log policy is defined.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
}
