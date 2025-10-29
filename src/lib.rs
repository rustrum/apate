mod config;
mod deceit;
mod matchers;

use std::collections::HashMap;
use std::net::Ipv4Addr;

use actix_router::{Path, ResourceDef};
use actix_web::App;
use actix_web::middleware::Logger;
use actix_web::{
    HttpRequest, HttpResponse, HttpServer,
    web::{self, Bytes, Data},
};
use async_lock::RwLock;

use crate::config::{ApateSpecs, AppConfig};

pub const DEFAULT_PORT: u16 = 8545;
const DEFAULT_RUST_LOG: &str = "info,apate=debug";

const ADMIN_API: &str = "/apate";
const ADMIN_API_PREPEND: &str = "/apate/prepend";
const ADMIN_API_REPLACE: &str = "/apate/replace";

pub struct ApateContext {
    pub specs: RwLock<ApateSpecs>,
}

impl ApateContext {
    pub fn new(specs: ApateSpecs) -> Self {
        Self {
            specs: RwLock::new(specs),
        }
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

pub fn apate_init_config(
    port: Option<u16>,
    log: Option<String>,
    files: Vec<String>,
) -> anyhow::Result<AppConfig> {
    let rust_log = log.unwrap_or(DEFAULT_RUST_LOG.to_string());

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(rust_log)).init();

    AppConfig::try_new(port, files)
}

pub async fn apate_run(config: AppConfig) -> std::io::Result<()> {
    if config.specs.deceit.is_empty() {
        log::warn!("Starting server without deceits in specs");
    }

    let data: Data<ApateContext> = Data::new(ApateContext::new(config.specs));

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone()) // Share config with handlers
            .wrap(Logger::default()) // Add logging middleware
            .default_service(web::to(process_request))
    })
    .bind((Ipv4Addr::UNSPECIFIED, config.port))?
    .keep_alive(actix_web::http::KeepAlive::Disabled)
    .run()
    .await
}

pub async fn process_request(
    req: HttpRequest,
    body: Bytes,
    state: Data<ApateContext>,
) -> HttpResponse {
    let path = req.path().to_string();

    if path.starts_with(ADMIN_API) {
        log::debug!("Admin API requested: {}", path);
        return admin_api_handler(&req, &body, &state).await;
    }

    return deceit_api_handler(&req, &body, &state).await;
}

async fn deceit_api_handler(
    req: &HttpRequest,
    body: &Bytes,
    state: &Data<ApateContext>,
) -> HttpResponse {
    // TODO deal with unwrap
    let deceit = &state.specs.read().await.deceit;

    let mut args_query: HashMap<String, String> = Default::default();
    let qstring = req.uri().query().unwrap_or_default();
    if let Ok(qargs) = serde_urlencoded::from_str::<HashMap<String, String>>(qstring) {
        args_query = qargs;
    } else {
        log::error!("Can't decode query string from URL");
    }

    for d in deceit {
        let mut path = Path::new(req.path());

        let resource = ResourceDef::new(d.uris.clone());
        if !resource.capture_match_info(&mut path) {
            continue;
        }

        let args_path = path.iter().collect();

        let ctx = RequestContext {
            req,
            body,
            path: &path,
            args_query: &args_query,
            args_path: &args_path,
        };

        log::debug!("{ctx:#?}");

        let Some(idx) = d.has_match(&ctx) else {
            continue;
        };

        return match d.process_response(idx, &ctx) {
            Ok(good) => good,
            Err(e) => HttpResponse::InternalServerError().body(format!("It happened! {e}\n")),
        };
    }

    HttpResponse::NotFound().body(format!(
        "Nothing can handle your requiest with path: {}\n",
        req.path()
    ))
}

async fn admin_api_handler(
    req: &HttpRequest,
    body: &Bytes,
    state: &Data<ApateContext>,
) -> HttpResponse {
    let path = req.path().to_string();
    if path == ADMIN_API {
        let specs = state.specs.read().await;

        return match toml::to_string(&*specs) {
            Ok(toml) => HttpResponse::Ok()
                .insert_header(("Content-Type", "application/x-toml"))
                .body(toml),
            Err(err) => HttpResponse::InternalServerError().body(format!("Serialize? No! {err}")),
        };
    }

    if path == ADMIN_API_PREPEND || path == ADMIN_API_REPLACE {
        let body_str = String::from_utf8_lossy(body);
        // log::trace!("New specs submitted:\n{}", body_str);

        match toml::from_str::<ApateSpecs>(&body_str) {
            Ok(new_specs) => {
                log::debug!("New specs: {:?}", new_specs);

                // let state = req.app_data::<web::Data<RwLock<Handlers>>>().unwrap();
                let mut specs = state.specs.write().await;

                log::trace!("Before update: {:?}", *specs);

                if path == ADMIN_API_PREPEND {
                    let mut deceit = new_specs.deceit;
                    deceit.extend(specs.deceit.clone());
                    specs.deceit = deceit;

                    log::debug!("After extend: {:?}", *specs);
                    return HttpResponse::Ok().body("Specs extended with an input TOML");
                } else {
                    *specs = new_specs;
                    log::debug!("After replace: {:?}", *specs);
                    return HttpResponse::Ok().body("Specs replaced with and input TOML");
                }
            }
            Err(e) => {
                return HttpResponse::BadRequest()
                    .body(format!("Failed to parse TOML from request body: {e:?}"));
            }
        };
    }

    HttpResponse::NotFound().body(format!("Admin API not available at provided path: {path}"))
}
