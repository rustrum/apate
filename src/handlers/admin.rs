use actix_web::{
    HttpRequest, HttpResponse, get, post,
    web::{Bytes, Data, ServiceConfig},
};
use serde::Serialize;

use crate::{ApateSpecs, ApateState};

pub const ADMIN_API: &str = "/apate";

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct ApateInfo<'a> {
    name: &'a str,
    version: &'a str,
}

pub fn admin_service_config(cfg: &mut ServiceConfig) {
    cfg.service(apate_info)
        .service(specification_get)
        .service(specification_replace)
        .service(specification_append)
        .service(specification_prepend);
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
            .insert_header(("Content-Type", "application/x-toml"))
            .body(toml),
        Err(err) => {
            HttpResponse::InternalServerError().body(format!("Serialize? Not able to! {err}"))
        }
    }
}

#[post("/replace")]
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

    specs.deceit = new_specs.deceit;

    HttpResponse::Ok().body("Specification replaced".to_string())
}

#[post("/prepend")]
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

    let mut deceit = new_specs.deceit;
    deceit.extend(specs.deceit.clone());
    specs.deceit = deceit;

    HttpResponse::Ok().body("New specification prepended to the existing one".to_string())
}

#[post("/append")]
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

    specs.deceit.extend(new_specs.deceit);

    HttpResponse::Ok().body("New specification appended to the existing one".to_string())
}

fn parse_input_toml(body: &Bytes) -> Result<ApateSpecs, HttpResponse> {
    let body_str = String::from_utf8_lossy(body);

    toml::from_str::<ApateSpecs>(&body_str).map_err(|e| {
        HttpResponse::BadRequest().body(format!("Failed to parse TOML from request body: {e:?}"))
    })
}
