use crate::actix_init_server;
pub use crate::config::{ApateSpecs, AppConfig, AppConfigBuilder};
pub use crate::deceit::{Deceit, DeceitBuilder, DeceitResponse, DeceitResponseBuilder};
pub use crate::matchers::Matcher;

pub use crate::{DEFAULT_PORT, DEFAULT_RUST_LOG};

pub struct ApateTestServer {
    #[allow(dead_code)]
    handle: std::thread::JoinHandle<Result<(), std::io::Error>>,
}

impl ApateTestServer {
    /// Start a test server with the given configuration.
    /// Arguments:
    /// * `config`: The configuration for the server.
    /// * `log`: The log level to use for the server, could reuse the `DEFAULT_RUST_LOG` value.
    /// * `start_delay_ms`: Delay after server start (for slow environments).
    pub fn start(config: AppConfig, log: &str, start_delay_ms: usize) -> ApateTestServer {
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

        if start_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(start_delay_ms as u64));
        }

        Self { handle }
    }
}
