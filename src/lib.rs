pub mod deceit;
mod handlers;
pub mod matchers;
mod output;
pub mod processors;
pub mod test;

use deceit::Deceit;

use std::collections::HashMap;
use std::io::Read as _;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use actix_router::Path;
use actix_web::App;
use actix_web::dev::Server;
use actix_web::middleware::Logger;
use actix_web::{
    HttpRequest, HttpServer,
    web::{self, Bytes, Data},
};
use async_lock::RwLock;
use serde::{Deserialize, Serialize};

use crate::processors::ApateProcessor;

pub const DEFAULT_PORT: u16 = 8228;
pub const DEFAULT_RUST_LOG: &str = "info,apate=debug";

#[derive(Debug)]
pub struct ApateConfig {
    pub port: u16,
    pub processors: HashMap<String, ApateProcessor>,
    pub specs: ApateSpecs,
}

impl Default for ApateConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            specs: Default::default(),
            processors: Default::default(),
        }
    }
}

impl ApateConfig {
    pub fn try_new_defaults() -> color_eyre::Result<Self> {
        Self::try_new(Some(DEFAULT_PORT), Vec::new())
    }

    pub fn try_new(port: Option<u16>, specs_files: Vec<String>) -> color_eyre::Result<Self> {
        let port = if let Some(p) = port {
            p
        } else {
            std::env::var("APATHE_PORT")
                .map(|p| p.parse::<u16>().unwrap())
                .unwrap_or(DEFAULT_PORT)
        };

        let specs = Self::read_specs(specs_files)?;

        Ok(ApateConfig {
            port,
            specs,
            processors: Default::default(),
        })
    }

    fn read_specs(specs_files: Vec<String>) -> color_eyre::Result<ApateSpecs> {
        let mut specs = ApateSpecs::default();

        for path in specs_files {
            let stub = Self::parse_specs_from(&path)?;
            specs.deceit.extend(stub.deceit);
        }

        for path in Self::read_paths_from_env() {
            let stub = Self::parse_specs_from(&path)?;
            specs.deceit.extend(stub.deceit);
        }
        Ok(specs)
    }

    fn parse_specs_from(path: &str) -> color_eyre::Result<ApateSpecs> {
        log::debug!("Parsing TOML config from: {}", path);

        let mut file = std::fs::File::open(path)
            .map_err(|e| color_eyre::eyre::eyre!("Can't parse {path}. {e}"))?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let specs: ApateSpecs = toml::from_slice(&buf)?;
        Ok(specs)
    }

    fn read_paths_from_env() -> Vec<String> {
        std::env::vars()
            .filter_map(|(key, value)| {
                if key.starts_with("APATHE_SPECS_FILE") {
                    Some(value)
                } else {
                    None
                }
            })
            .collect()
    }

    fn into_state(self) -> ApateState {
        ApateState {
            specs: RwLock::new(self.specs),
            counters: Default::default(),
            processors: self.processors,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApateSpecs {
    pub deceit: Vec<Deceit>,
}

/// Shared state for apate web server.
pub struct ApateState {
    pub specs: RwLock<ApateSpecs>,
    pub counters: ApateCounters,
    pub processors: HashMap<String, ApateProcessor>,
}

#[derive(Clone, Default)]
pub struct ApateCounters {
    counters: Arc<std::sync::RwLock<HashMap<String, Arc<AtomicU64>>>>,
}

impl ApateCounters {
    pub fn get_or_default(&self, key: &str) -> color_eyre::Result<Arc<AtomicU64>> {
        let mut counters = self
            .counters
            .write()
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

        let counter = counters.entry(key.to_string()).or_default();
        Ok(counter.clone())
    }

    pub fn get_and_increment(&self, key: &str) -> color_eyre::Result<u64> {
        let mut counters = self
            .counters
            .write()
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

        let counter = counters.entry(key.to_string()).or_default();
        let prev_value = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(prev_value)
    }
}

#[derive(Debug)]
pub struct RequestContext<'a> {
    pub req: &'a HttpRequest,
    pub body: &'a Bytes,
    pub path: &'a Path<&'a str>,
    pub args_query: &'a HashMap<String, String>,
    pub args_path: &'a HashMap<&'a str, &'a str>,
}

/// Create and run apate server based on input config.
pub async fn apate_server_run(config: ApateConfig) -> std::io::Result<()> {
    init_actix_web_server(config)?.await
}

/// Initialize server configuration with overrides.
/// All arguments to this function will override configuration from ENV variables
pub fn apate_init_server_config(
    port: Option<u16>,
    log: Option<String>,
    files: Vec<String>,
) -> color_eyre::Result<ApateConfig> {
    let rust_log = log.unwrap_or(DEFAULT_RUST_LOG.to_string());

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(rust_log)).init();

    ApateConfig::try_new(port, files)
}

fn init_actix_web_server(config: ApateConfig) -> std::io::Result<Server> {
    if config.specs.deceit.is_empty() {
        log::warn!("Starting server without deceits in specs");
    }
    let port = config.port;

    let data: Data<ApateState> = Data::new(config.into_state());

    let server = HttpServer::new(move || {
        let mut app = App::new().app_data(data.clone()).wrap(Logger::default());
        #[cfg(feature = "server")]
        {
            app = app
                .service(web::scope(handlers::ADMIN_API).configure(handlers::admin_service_config));
        }
        app.default_service(web::to(handlers::apate_server_handler))
    })
    .bind((Ipv4Addr::UNSPECIFIED, port))?
    .keep_alive(actix_web::http::KeepAlive::Disabled)
    .run();

    Ok(server)
}

pub struct ApateConfigBuilder {
    port: u16,
    deceit: Vec<Deceit>,
    pub processors: HashMap<String, ApateProcessor>,
}

impl Default for ApateConfigBuilder {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            deceit: Default::default(),
            processors: Default::default(),
        }
    }
}

impl ApateConfigBuilder {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn add_deceit(mut self, deceit: Deceit) -> Self {
        self.deceit.push(deceit);
        self
    }

    pub fn register_processor(mut self, processor: ApateProcessor) -> Self {
        self.processors.insert(processor.id.clone(), processor);
        self
    }

    pub fn build(self) -> ApateConfig {
        ApateConfig {
            port: self.port,
            specs: ApateSpecs {
                deceit: self.deceit,
            },
            processors: self.processors,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    const TOML_TEST: &str = include_str!("../examples/apate-specs.toml");

    /// Just checking that examples toml is valid
    #[test]
    fn deserialize_examples_toml() {
        let specs: ApateSpecs = toml::from_str(TOML_TEST).unwrap();

        // The coolest debug approach ever
        println!("{specs:#?}");
    }
}
