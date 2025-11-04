pub mod deceit;
mod handlers;
pub mod matchers;
mod output;
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

pub const DEFAULT_PORT: u16 = 8228;
pub const DEFAULT_RUST_LOG: &str = "info,apate=debug";

const ADMIN_API: &str = "/apate";
const ADMIN_API_PREPEND: &str = "/apate/prepend";
const ADMIN_API_REPLACE: &str = "/apate/replace";

#[derive(Debug)]
pub struct AppConfig {
    pub port: u16,
    pub specs: ApateSpecs,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            specs: Default::default(),
        }
    }
}

impl AppConfig {
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

        Ok(AppConfig {
            port,
            specs,
            // rust_log: env::var("RUST_LOG").unwrap_or("info,api_stub_server=debug".into()),
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
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApateSpecs {
    pub deceit: Vec<Deceit>,
}

/// Shared state for apate web server.
pub struct ApateState {
    pub specs: RwLock<ApateSpecs>,
    pub counters: ApateCounters,
}

impl ApateState {
    pub fn new(specs: ApateSpecs) -> Self {
        Self {
            specs: RwLock::new(specs),
            counters: Default::default(),
        }
    }
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

pub fn apate_server_init_config(
    port: Option<u16>,
    log: Option<String>,
    files: Vec<String>,
) -> color_eyre::Result<AppConfig> {
    let rust_log = log.unwrap_or(DEFAULT_RUST_LOG.to_string());

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(rust_log)).init();

    AppConfig::try_new(port, files)
}

pub async fn apate_run(config: AppConfig) -> std::io::Result<()> {
    actix_init_server(config)?.await
}

fn actix_init_server(config: AppConfig) -> std::io::Result<Server> {
    if config.specs.deceit.is_empty() {
        log::warn!("Starting server without deceits in specs");
    }

    let data: Data<ApateState> = Data::new(ApateState::new(config.specs));

    let server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone()) // Share config with handlers
            .wrap(Logger::default()) // Add logging middleware
            .default_service(web::to(handlers::apate_server_handler))
    })
    .bind((Ipv4Addr::UNSPECIFIED, config.port))?
    .keep_alive(actix_web::http::KeepAlive::Disabled)
    .run();

    Ok(server)
}

pub struct AppConfigBuilder {
    port: u16,
    deceit: Vec<Deceit>,
}

impl Default for AppConfigBuilder {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            deceit: Default::default(),
        }
    }
}

impl AppConfigBuilder {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn add_deceit(mut self, deceit: Deceit) -> Self {
        self.deceit.push(deceit);
        self
    }

    pub fn build(self) -> AppConfig {
        AppConfig {
            port: self.port,
            specs: ApateSpecs {
                deceit: self.deceit,
            },
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
