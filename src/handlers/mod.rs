//! This module has server logic to handle all URI requests.

#[cfg(feature = "server")]
mod admin;

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

    let mut ctx = RequestContext::new(req, body);

    for (deceit_idx, d) in deceit.iter().enumerate() {
        let Some(path) = d.match_againtst_uris(&ctx.request_path) else {
            continue;
        };

        let args_path = path
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        ctx.update_paths(path.as_str().to_string(), args_path);

        log::trace!("Request context is: {ctx:?}");

        let deceit_ref = ResourceRef::new(deceit_idx);
        let Some(idx) = d.match_response(&deceit_ref, &ctx, &state.rhai) else {
            continue;
        };

        log::debug!("Deceit {deceit_ref} matched (^_^). Processing response: {idx}");

        let Some(response) = d.responses.get(idx) else {
            log::error!("Wow we definitely must have response for this index {idx}");
            continue;
        };

        // Here all matchers checks passed
        // Now we are processing response
        // At this point we can't skip to the next deceit anymore
        let drctx = match create_responce_context(
            d,
            ctx.clone(),
            state.counters.clone(),
            state.minijinja.clone(),
        ) {
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

                match apply_processors(
                    &deceit_ref,
                    &state.processors,
                    &prcs,
                    &drctx,
                    &body,
                    &state.rhai,
                ) {
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
        ctx.request_path
    ))
}
