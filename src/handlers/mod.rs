//! This module has server logic to handle all URI requests.

#[cfg(feature = "server")]
mod admin;

use std::sync::atomic::Ordering;

#[cfg(feature = "server")]
pub use admin::{ADMIN_API, admin_service_config};

use actix_web::{
    HttpRequest, HttpResponse, HttpResponseBuilder,
    http::StatusCode,
    web::{Bytes, Data},
};

use crate::{
    ApateState, RequestContext, ResourceRef,
    deceit::{DEFAULT_RESPONSE_CODE, create_response_context},
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

        let Some(dresp) = d.responses.get(idx) else {
            log::error!("Wow we definitely must have response for this index {idx}");
            continue;
        };

        // Here all matchers checks passed
        // Now we are processing response
        // At this point we can't skip to the next deceit anymore
        let drctx = match create_response_context(ctx.clone(), state.counters.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .body(format!("Cant create deceit context! {e}"));
            }
        };

        let output_body = crate::output::output_response_body(
            &deceit_ref,
            dresp.output_type,
            &dresp.output,
            &drctx,
            &state.minijinja,
            &state.rhai,
        );

        return match output_body {
            Ok(body) => {
                let mut prcs = Vec::with_capacity(d.processors.len() + dresp.processors.len());
                prcs.extend(d.processors.iter());
                prcs.extend(dresp.processors.iter());

                match apply_processors(
                    &deceit_ref,
                    &state.processors,
                    &prcs,
                    &drctx,
                    &body,
                    &state.rhai,
                ) {
                    Ok(new_body) => {
                        let mut hrb = HttpResponseBuilder::new(DEFAULT_RESPONSE_CODE);
                        insert_response_headers(&mut hrb, &d.headers, &dresp.headers);
                        if let Ok(code) =
                            StatusCode::from_u16(drctx.response_code.load(Ordering::Relaxed))
                        {
                            // This is where we are applying new status code
                            hrb.status(code);
                        }

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

/* impl DeceitResponse {
    pub fn prepare(
        &self,
        deceit: &Deceit,
        drctx: &DeceitResponseContext,
    ) -> color_eyre::Result<(HttpResponseBuilder, Vec<u8>)> {
        let output_body =
            crate::output::build_response_body(self.output_type, &self.output, drctx)?;

        let mut hrb = HttpResponseBuilder::new(StatusCode::from_u16(DEFAULT_RESPONSE_CODE)?);

        insert_response_headers(&mut hrb, &deceit.headers, &self.headers);

        Ok((hrb, output_body))
    }
} */

fn insert_response_headers(
    rbuilder: &mut HttpResponseBuilder,
    parent_headers: &[(String, String)],
    headers: &[(String, String)],
) {
    for (k, v) in parent_headers {
        rbuilder.insert_header((k.as_str(), v.as_str()));
    }
    for (k, v) in headers {
        rbuilder.insert_header((k.as_str(), v.as_str()));
    }
}
