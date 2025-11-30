use actix_web::{
    HttpRequest, HttpResponse, get,
    http::header::CONTENT_TYPE,
    post, routes,
    web::{self, Bytes, Data, ServiceConfig},
};
use include_dir::{Dir, include_dir};
use serde::Serialize;

use crate::{ApateSpecs, ApateState};

pub const ADMIN_API: &str = "/apate";

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

const ASSETS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

#[derive(Serialize)]
struct ApateInfo<'a> {
    name: &'a str,
    version: &'a str,
}

pub fn admin_service_config(cfg: &mut ServiceConfig) {
    cfg.service(apate_ui)
        .service(apate_info)
        .service(specification_get)
        .service(specification_replace)
        .service(specification_append)
        .service(specification_prepend)
        .service(admin_assets);
}

#[routes]
#[get("")]
#[get("{path:/*}")]
async fn apate_ui() -> HttpResponse {
    if let Some(file) = ASSETS_DIR.get_file("index.html") {
        let body = web::Bytes::copy_from_slice(file.contents());
        HttpResponse::Ok()
            .insert_header((CONTENT_TYPE, "text/html"))
            .body(body)
    } else {
        HttpResponse::NotFound().body("WTF index.html. This must not happen.".to_string())
    }
}

#[get("/info")]
async fn apate_info() -> HttpResponse {
    let info = ApateInfo {
        name: "Apate API mocking server",
        version: PKG_VERSION,
    };

    match serde_json::to_string(&info) {
        Ok(json) => HttpResponse::Ok()
            .insert_header(("Content-Type", "application/json"))
            .body(json),
        Err(err) => {
            HttpResponse::InternalServerError().body(format!("Serialize? Not able to! {err}"))
        }
    }
}

#[get("/specs")]
async fn specification_get(state: Data<ApateState>) -> HttpResponse {
    let specs = state.specs.read().await;

    match toml::to_string(&*specs) {
        Ok(toml) => HttpResponse::Ok()
            .insert_header(("Content-Type", "text/x-toml"))
            .body(toml),
        Err(err) => {
            HttpResponse::InternalServerError().body(format!("Serialize? Not able to! {err}"))
        }
    }
}

#[post("/specs/replace")]
async fn specification_replace(
    _req: HttpRequest,
    body: Bytes,
    state: Data<ApateState>,
) -> HttpResponse {
    let new_specs = match parse_input_toml(&body) {
        Ok(specs) => specs,
        Err(err_response) => return err_response,
    };

    let mut specs = state.specs.write().await;
    *specs = new_specs;

    state.clear_cache();
    state.lua.clear_and_update(specs.lua.clone());

    HttpResponse::Ok().body("Specification replaced".to_string())
}

#[post("/specs/prepend")]
async fn specification_prepend(
    _req: HttpRequest,
    body: Bytes,
    state: Data<ApateState>,
) -> HttpResponse {
    let new_specs = match parse_input_toml(&body) {
        Ok(specs) => specs,
        Err(err_response) => return err_response,
    };

    let mut specs = state.specs.write().await;

    specs.prepend(new_specs);

    state.clear_cache();
    state.lua.clear_and_update(specs.lua.clone());

    HttpResponse::Ok().body("New specification prepended to the existing one".to_string())
}

#[post("/specs/append")]
async fn specification_append(
    _req: HttpRequest,
    body: Bytes,
    state: Data<ApateState>,
) -> HttpResponse {
    let new_specs = match parse_input_toml(&body) {
        Ok(specs) => specs,
        Err(err_response) => return err_response,
    };

    let mut specs = state.specs.write().await;

    specs.append(new_specs);

    state.clear_cache();
    state.lua.clear_and_update(specs.lua.clone());

    HttpResponse::Ok().body("New specification appended to the existing one".to_string())
}

fn parse_input_toml(body: &Bytes) -> Result<ApateSpecs, HttpResponse> {
    let body_str = String::from_utf8_lossy(body);

    toml::from_str::<ApateSpecs>(&body_str).map_err(|e| {
        HttpResponse::BadRequest().body(format!("Failed to parse TOML from request body: {e:?}"))
    })
}

#[get("/assets/{filename:.*}")]
async fn admin_assets(path: web::Path<String>) -> HttpResponse {
    let filename = path.into_inner();

    if let Some(file) = ASSETS_DIR.get_file(&filename) {
        let body = web::Bytes::copy_from_slice(file.contents());
        // let content_type = mime_guess::from_path(&filename).first_or_octet_stream();
        HttpResponse::Ok()
            // .insert_header((header::CONTENT_TYPE, content_type.as_ref()))
            .body(body)
    } else {
        HttpResponse::NotFound().body(format!("File not found: {filename}"))
    }
}
