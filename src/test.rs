use crate::actix_init_server;
pub use crate::config::{ApateSpecs, AppConfig, AppConfigBuilder};
pub use crate::deceit::{Deceit, DeceitBuilder, DeceitResponse, DeceitResponseBuilder};
pub use crate::matchers::Matcher;

pub use crate::DEFAULT_RUST_LOG;

pub struct ApateTestServer {
    #[allow(dead_code)]
    handle: std::thread::JoinHandle<Result<(), std::io::Error>>,
}

pub fn apate_run_test(config: AppConfig, log: &str) -> ApateTestServer {
    if !log.is_empty() {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log)).init();
    }
    if config.specs.deceit.is_empty() {
        log::warn!("Starting server without deceits in specs");
    }

    let handle = std::thread::spawn(move || {
        let server = actix_init_server(config).expect("Test server must be initialized");
        actix_web::rt::Runtime::new()
            .expect("Runtime expected")
            .block_on(server)
    });

    ApateTestServer { handle }
}
