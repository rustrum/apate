//! This module has server logic to handle all URI requests.

#[cfg(feature = "server")]
mod admin;

use std::{collections::HashMap, rc::Rc};

use actix_router::Path;
#[cfg(feature = "server")]
pub use admin::{ADMIN_API, admin_service_config};

use actix_web::{
    HttpRequest, HttpResponse,
    web::{Bytes, Data},
};

use crate::{
    ApateState, RequestContext, ResourceRef, deceit::create_responce_context,
    processors::apply_processors,
};

/// Handle all apate server requests
pub async fn apate_server_handler(
    req: HttpRequest,
    body: Bytes,
    state: Data<ApateState>,
) -> HttpResponse {
    deceit_handler(req, body, state).await
}

async fn deceit_handler(req: HttpRequest, body: Bytes, state: Data<ApateState>) -> HttpResponse {
    let deceit = &state.specs.read().await.deceit;

    let mut args_query: HashMap<String, String> = Default::default();
    let qstring = req.uri().query().unwrap_or_default();
    if let Ok(qargs) = serde_urlencoded::from_str::<HashMap<String, String>>(qstring) {
        args_query = qargs;
    } else {
        log::error!("Can't decode query string from URL");
    }

    let mut ctx = RequestContext {
        req: Rc::new(req),
        body: Rc::new(body),
        path: Rc::new(Path::new("/".to_string())),
        args_query: Rc::new(args_query),
        args_path: Rc::new(Default::default()),
    };

    for (deceit_idx, d) in deceit.iter().enumerate() {
        let Some(path) = d.match_againtst_uris(ctx.req.path()) else {
            continue;
        };

        let args_path = path
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        ctx.update_paths(path, args_path);

        log::trace!("Request context is: {ctx:?}");

        let deceit_ref = ResourceRef::new(deceit_idx);
        let Some(idx) = d.match_response(&deceit_ref, &ctx, &state.lua) else {
            continue;
        };

        log::debug!("Deceit successful matched (^_^). Processing response: {idx}");

        let Some(response) = d.responses.get(idx) else {
            log::error!("Wow we definitely must have response for this index {idx}");
            continue;
        };

        // Here all matchers checks passed
        // Now we are processing response
        // At this point we can't skip to the next deceit anymore
        let drctx = match create_responce_context(d, &ctx, &state.counters, &state.minijinja) {
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
        ctx.req.path()
    ))
}
