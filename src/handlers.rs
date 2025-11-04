//! This module has server logic to handle all URI requests.

use std::collections::HashMap;

use actix_web::{
    HttpRequest, HttpResponse,
    web::{Bytes, Data},
};

use crate::{
    ADMIN_API, ADMIN_API_PREPEND, ADMIN_API_REPLACE, ApateSpecs, ApateState, RequestContext,
    deceit::create_responce_context, processors::apply_processors,
};

/// Handle all apate server requests
pub async fn apate_server_handler(
    req: HttpRequest,
    body: Bytes,
    state: Data<ApateState>,
) -> HttpResponse {
    let path = req.path().to_string();

    if path.starts_with(ADMIN_API) {
        log::debug!("Admin API requested: {}", path);
        return admin_handler(&req, &body, &state).await;
    }

    return deceit_handler(&req, &body, &state).await;
}

async fn deceit_handler(req: &HttpRequest, body: &Bytes, state: &Data<ApateState>) -> HttpResponse {
    let deceit = &state.specs.read().await.deceit;

    let mut args_query: HashMap<String, String> = Default::default();
    let qstring = req.uri().query().unwrap_or_default();
    if let Ok(qargs) = serde_urlencoded::from_str::<HashMap<String, String>>(qstring) {
        args_query = qargs;
    } else {
        log::error!("Can't decode query string from URL");
    }

    for d in deceit {
        let Some(path) = d.match_againtst_uris(req.path()) else {
            continue;
        };

        let args_path = path.iter().collect();

        let ctx = RequestContext {
            req,
            body,
            path: &path,
            args_query: &args_query,
            args_path: &args_path,
        };

        log::trace!("Request context is: {ctx:?}");

        let Some(idx) = d.match_response(&ctx) else {
            continue;
        };

        log::debug!("Deceit successful matched (^_^). Processing response: {idx}");

        let Some(response) = d.responses.get(idx) else {
            log::error!("Wow we definitely must have response for this index {idx}");
            continue;
        };

        // At tis point all matchers checks passed
        let drctx = match create_responce_context(d, &ctx, &state.counters) {
            Ok(ctx) => ctx,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .body(format!("Cant create deceit context! {e}"));
            }
        };

        return match response.prepare(d, &drctx) {
            Ok((mut hrb, body)) => {
                let mut prcs = Vec::with_capacity(d.processors.len() + response.processors.len());
                prcs.extend(d.processors.iter());
                prcs.extend(response.processors.iter());

                match apply_processors(&state.processors, &prcs, &drctx, &body) {
                    Ok(new_body) => {
                        if let Some(bts) = new_body {
                            hrb.body(bts)
                        } else {
                            hrb.body(body)
                        }
                    }
                    Err(e) => HttpResponse::InternalServerError()
                        .body(format!("Can't apply post processors! {e}\n")),
                }
            }
            Err(e) => HttpResponse::InternalServerError().body(format!("It happened! {e}\n")),
        };
    }

    HttpResponse::NotFound().body(format!(
        "Nothing can handle your requiest with path: {}\n",
        req.path()
    ))
}

async fn admin_handler(req: &HttpRequest, body: &Bytes, state: &Data<ApateState>) -> HttpResponse {
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
