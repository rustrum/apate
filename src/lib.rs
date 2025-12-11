pub mod deceit;
mod handlers;
pub mod jinja;
pub mod matchers;
pub mod output;
pub mod processors;
pub mod rhai;
pub mod test;

use deceit::Deceit;

use std::collections::HashMap;
use std::fmt::Display;
use std::io::Read as _;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use actix_web::App;
use actix_web::dev::Server;
use actix_web::middleware::Logger;
use actix_web::{
    HttpRequest, HttpServer,
    web::{self, Bytes, Data},
};
use async_lock::RwLock;
use serde::{Deserialize, Serialize};

use crate::jinja::MiniJinjaState;
use crate::processors::ApateProcessor;
use crate::rhai::{RhaiScript, RhaiState};

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
            let parsed = Self::parse_specs_from(&path)?;
            specs.append(parsed);
        }

        for path in Self::read_paths_from_env() {
            let parsed = Self::parse_specs_from(&path)?;
            specs.append(parsed);
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
        let rhai = RhaiState::default();
        rhai.clear_and_update(self.specs.rhai.clone());
        ApateState {
            specs: RwLock::new(self.specs),
            processors: self.processors,
            rhai,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApateSpecs {
    #[serde(default)]
    pub rhai: Vec<RhaiScript>,
    #[serde(default)]
    pub deceit: Vec<Deceit>,
}

impl ApateSpecs {
    pub fn append(&mut self, specs: ApateSpecs) {
        self.deceit.extend(specs.deceit);
        self.rhai.extend(specs.rhai);
    }

    pub fn prepend(&mut self, mut specs: ApateSpecs) {
        specs.deceit.extend(self.deceit.clone());
        specs.rhai.extend(self.rhai.clone());

        self.deceit = specs.deceit;
        self.rhai = specs.rhai;
    }
}

/// Shared state for apate web server.
#[derive(Default)]
pub struct ApateState {
    pub specs: RwLock<ApateSpecs>,
    pub counters: ApateCounters,
    pub processors: HashMap<String, ApateProcessor>,
    pub minijinja: MiniJinjaState,
    pub rhai: RhaiState,
}

impl ApateState {
    pub fn clear_cache(&self) {
        self.minijinja.clear();
    }
}

#[derive(Clone, Default)]
pub struct ApateCounters {
    counters: Arc<std::sync::RwLock<HashMap<String, Arc<AtomicU64>>>>,
}

impl ApateCounters {
    pub fn get_or_default(&self, key: &str) -> color_eyre::Result<u64> {
        let mut counters = self
            .counters
            .write()
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

        let counter = counters.entry(key.to_string()).or_default();
        Ok(counter.load(std::sync::atomic::Ordering::SeqCst))
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

#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Maybe I even do not need to store it during request anymore
    // req: Rc<HttpRequest>,
    pub body: Arc<Bytes>,
    pub method: String,
    pub request_path: Arc<String>,
    pub headers: Arc<HashMap<String, String>>,
    pub path: Arc<String>,
    pub query_args: Arc<HashMap<String, String>>,
    pub path_args: Arc<HashMap<String, String>>,
}

impl RequestContext {
    pub fn new(req: HttpRequest, body: Bytes) -> Self {
        let method = req.method().to_string();
        let headers = req
            .headers()
            .iter()
            .filter_map(|(k, v)| match v.to_str().map(|v| v.to_string()) {
                Ok(value) => Some((k.to_string(), value)),
                Err(e) => {
                    log::warn!("Can't convert header value to string by key: {k} {e}");
                    None
                }
            })
            .collect();

        let mut args_query: HashMap<String, String> = Default::default();
        let qstring = req.uri().query().unwrap_or_default();
        if let Ok(qargs) = serde_urlencoded::from_str::<HashMap<String, String>>(qstring) {
            args_query = qargs;
        } else {
            log::error!("Can't decode query string from URL");
        }
        let request_path = Arc::new(req.path().to_string());
        // let req = Rc::new(req);
        Self {
            // req,
            body: Arc::new(body),
            method,
            request_path,
            headers: Arc::new(headers),
            query_args: Arc::new(args_query),
            path: Arc::new("/".to_string()),
            path_args: Arc::new(Default::default()),
        }
    }

    pub fn update_paths(&mut self, path: String, args_path: HashMap<String, String>) {
        self.path = Arc::new(path);
        self.path_args = Arc::new(args_path);
    }
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
    scripts: HashMap<String, String>,
}

impl Default for ApateConfigBuilder {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            deceit: Default::default(),
            processors: Default::default(),
            scripts: Default::default(),
        }
    }
}

impl ApateConfigBuilder {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn add_script(mut self, id: &str, script: &str) -> Self {
        self.scripts.insert(id.to_string(), script.to_string());
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
                rhai: self
                    .scripts
                    .into_iter()
                    .map(|(id, script)| RhaiScript { id, script })
                    .collect(),
            },
            processors: self.processors,
        }
    }
}

/// Represents resource path in the configuration.
/// It's just ids of the arrays we are iterating right now.
#[derive(Clone)]
pub struct ResourceRef {
    ids: Vec<usize>,
}

impl ResourceRef {
    pub fn new(top_level_id: usize) -> Self {
        Self {
            ids: vec![top_level_id],
        }
    }

    pub fn with_level(&self, id: usize) -> Self {
        let mut next = self.clone();
        next.ids.push(id);
        next
    }

    fn as_string(&self) -> String {
        self.ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<String>>()
            .join("-")
    }

    pub fn to_resource_id(&self, resource_type: &str) -> String {
        format!("{resource_type}:{}", self.as_string())
    }
}

impl Display for ResourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_string())
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
