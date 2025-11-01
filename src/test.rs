use actix_web::dev::ServerHandle;

use crate::actix_init_server;
pub use crate::config::{ApateSpecs, AppConfig, AppConfigBuilder};
pub use crate::deceit::{Deceit, DeceitBuilder, DeceitResponse, DeceitResponseBuilder};
pub use crate::matchers::Matcher;

pub use crate::{DEFAULT_PORT, DEFAULT_RUST_LOG};

/// Init env logger for apate server.
/// Call it with desired log level or use [`DEFAULT_RUST_LOG`] constant.
pub fn init_env_logger(log: &str) {
    if log.is_empty() {
        return;
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log)).init();
}

pub struct ApateTestServer {
    server_handle: ServerHandle,
    #[allow(dead_code)]
    handle: std::thread::JoinHandle<Result<(), std::io::Error>>,
}

impl Drop for ApateTestServer {
    fn drop(&mut self) {
        let stopping = self.server_handle.stop(false);

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let spawn_handle = handle.spawn(stopping);
            while spawn_handle.is_finished() {
                // It looks stupid but it work when running inside a Tokio runtime
                // I was not able to use something like blocks_on here
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        } else {
            // Not inside a Tokio runtime
            let trt = tokio::runtime::Runtime::new().unwrap();
            trt.block_on(stopping);
        }
    }
}

impl ApateTestServer {
    /// Start a test server with the given configuration.
    /// Arguments:
    /// * `config`: The configuration for the server.
    /// * `delay_ms`: Delay after server start to let slow envs to inintialize.
    pub fn start(config: AppConfig, delay_ms: usize) -> ApateTestServer {
        if config.specs.deceit.is_empty() {
            log::warn!("Starting server without deceits in specs");
        }

        let server = actix_init_server(config).expect("Test server must be initialized");
        let server_handle = server.handle();
        let handle = std::thread::spawn(move || {
            actix_web::rt::Runtime::new()
                .expect("Runtime expected")
                .block_on(server)
        });

        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms as u64));
        }

        Self {
            handle,
            server_handle,
        }
    }
}
