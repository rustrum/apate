//! Deceit is the unit responsible for processing serveral status URIs or path patters.
//! All deceit related logic is placed into this module.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
};

use actix_router::{Path, ResourceDef};
use actix_web::{HttpResponseBuilder, http::StatusCode};

use serde::{Deserialize, Serialize};

use crate::{
    ApateConfig, ApateCounters, RequestContext, ResourceRef,
    lua::LuaState,
    matchers::{Matcher, is_matcher_approves},
    output::{MiniJinjaState, OutputType},
    processors::Processor,
};

const DEFAULT_RESPONSE_CODE: u16 = 200;

/// Specification unit that applies to one or several URI paths.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Deceit {
    /// List of URIs that could be string prefixed with '/'
    /// or a pattern with arguments like '/user/{user_id}'.
    pub uris: Vec<String>,

    /// Common response headers for current configuration unit.
    #[serde(default)]
    pub headers: Vec<(String, String)>,

    /// Set of simple rules to run against input request.
    /// If one matcher fails - current deceit processing will be skipped.
    #[serde(default)]
    pub matchers: Vec<Matcher>,

    /// Parse request body as JSON and add it to the request context.
    #[serde(default)]
    pub json_request: bool,

    #[serde(default)]
    pub processors: Vec<Processor>,

    /// Responses that can be applied after deceit level checks/matchers completed.
    #[serde(default)]
    pub responses: Vec<DeceitResponse>,
}

impl Deceit {
    pub fn match_againtst_uris(&self, request_path: &str) -> Option<Path<String>> {
        log::debug!(
            "Checking path: {request_path} against deceit URIs: {:?}",
            self.uris
        );

        let mut path = Path::new(request_path.to_string());

        let resource = ResourceDef::new(self.uris.clone());

        if resource.capture_match_info(&mut path) {
            Some(path)
        } else {
            None
        }
    }

    pub fn match_response(
        &self,
        rref: &ResourceRef,
        ctx: &RequestContext,
        lua: &LuaState,
    ) -> Option<usize> {
        // Top level matchers
        for (mid, matcher) in self.matchers.iter().enumerate() {
            let matcher_ref = rref.with_level(mid);
            if !is_matcher_approves(&matcher_ref, matcher, ctx, lua) {
                return None;
            }
        }

        // Deceit level matchers
        for (idx, dr) in self.responses.iter().enumerate() {
            if dr.matchers.is_empty() {
                // Empty matchers - always yes
                return Some(idx);
            }
            let deceit_ref = rref.with_level(idx);
            for (mid, matcher) in dr.matchers.iter().enumerate() {
                let matcher_ref = deceit_ref.with_level(mid);
                if is_matcher_approves(&matcher_ref, matcher, ctx, lua) {
                    return Some(idx);
                }
            }
        }

        None
    }
}

/// Context for output renderers and pre/post processors as well.
#[derive(Serialize)]
pub struct DeceitResponseContext<'a> {
    pub path: &'a str,

    pub headers: HashMap<&'a str, &'a str>,

    pub query_args: &'a HashMap<String, String>,

    pub path_args: &'a HashMap<String, String>,

    pub request_json: Option<serde_json::Value>,

    #[serde(skip_serializing)]
    pub response_code: Arc<AtomicU16>,

    #[serde(skip_serializing)]
    pub counters: &'a ApateCounters,

    #[serde(skip_serializing)]
    pub minijinja: &'a MiniJinjaState,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeceitResponse {
    /// Code for this particular response
    #[serde(default)]
    pub code: Option<u16>,

    /// Same as for [`Deceit`] but it will check next response on a failure
    #[serde(default)]
    pub matchers: Vec<Matcher>,

    #[serde(default)]
    pub headers: Vec<(String, String)>,

    #[serde(default)]
    pub processors: Vec<Processor>,

    #[serde(default)]
    pub output_type: OutputType,

    #[serde(default)]
    pub output: String,
}

impl DeceitResponse {
    pub fn prepare<'a>(
        &self,
        deceit: &Deceit,
        drctx: &DeceitResponseContext<'a>,
    ) -> color_eyre::Result<(HttpResponseBuilder, Vec<u8>)> {
        let output_body =
            crate::output::build_response_body(self.output_type, &self.output, drctx)?;

        let response_code_from_tpl = drctx.response_code.load(Ordering::Relaxed);
        let response_code = if response_code_from_tpl > 0 {
            response_code_from_tpl
        } else {
            self.code.unwrap_or(DEFAULT_RESPONSE_CODE)
        };

        let mut hrb: HttpResponseBuilder =
            HttpResponseBuilder::new(StatusCode::from_u16(response_code)?);

        insert_response_headers(&mut hrb, &deceit.headers, &self.headers);

        Ok((hrb, output_body))
    }
}

pub fn create_responce_context<'a>(
    deceit: &'a Deceit,
    ctx: &'a RequestContext,
    cnt: &'a ApateCounters,
    minijinja: &'a MiniJinjaState,
) -> color_eyre::Result<DeceitResponseContext<'a>> {
    let mut headers = HashMap::new();
    for (k, v) in ctx.req.headers().iter() {
        headers.insert(k.as_str(), v.to_str()?);
    }

    let request_json = if deceit.json_request && !ctx.body.trim_ascii().is_empty() {
        let body = String::from_utf8_lossy(&ctx.body);
        Some(serde_json::from_slice::<serde_json::Value>(
            body.as_bytes(),
        )?)
    } else {
        None
    };

    Ok(DeceitResponseContext {
        path: ctx.req.path(),
        headers,
        query_args: &ctx.args_query,
        path_args: &ctx.args_path,
        request_json,
        response_code: Arc::new(AtomicU16::new(0)),
        counters: cnt,
        minijinja,
    })
}

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

pub struct DeceitBuilder {
    uris: Vec<String>,

    headers: Vec<(String, String)>,

    matchers: Vec<Matcher>,

    processors: Vec<Processor>,

    json_request: bool,

    responses: Vec<DeceitResponse>,
}

impl DeceitBuilder {
    pub fn with_uris<T: AsRef<str>>(uris: &[T]) -> Self {
        let uris = uris.iter().map(|u| u.as_ref().to_string()).collect();
        Self {
            uris,
            headers: Vec::new(),
            matchers: Vec::new(),
            json_request: false,
            responses: Vec::new(),
            processors: Vec::new(),
        }
    }

    pub fn build(self) -> Deceit {
        Deceit {
            uris: self.uris,
            headers: self.headers,
            matchers: self.matchers,
            json_request: self.json_request,
            processors: self.processors,
            responses: self.responses,
        }
    }

    /// Wraps single [`Deceit`] into a [`AppConfig`] with default parameters.
    pub fn to_app_config(self) -> ApateConfig {
        ApateConfig {
            specs: crate::ApateSpecs {
                deceit: vec![self.build()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Wraps single [`Deceit`] into a [`AppConfig`] with specified port.
    pub fn to_app_config_with_port(self, port: u16) -> ApateConfig {
        ApateConfig {
            port,
            specs: crate::ApateSpecs {
                deceit: vec![self.build()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn add_header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn add_processor(mut self, processor: Processor) -> Self {
        self.processors.push(processor);
        self
    }

    pub fn json_request(mut self, json_request: bool) -> Self {
        self.json_request = json_request;
        self
    }

    pub fn with_responses(mut self, responses: Vec<DeceitResponse>) -> Self {
        self.responses = responses;
        self
    }

    pub fn add_response(mut self, response: DeceitResponse) -> Self {
        self.responses.push(response);
        self
    }

    //
    // Matchers configuration
    //
    pub fn add_matcher(mut self, matcher: Matcher) -> Self {
        self.matchers.push(matcher);
        self
    }

    pub fn require_method(mut self, http_method: &str) -> Self {
        self.matchers.push(Matcher::Method {
            eq: http_method.to_string(),
        });
        self
    }

    pub fn require_header(mut self, key: &str, value: &str) -> Self {
        self.matchers.push(Matcher::Header {
            key: key.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn require_query_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::QueryArg {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn require_path_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::PathArg {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn require_json_match(mut self, json_path: &str, eq: &str) -> Self {
        self.matchers.push(Matcher::Json {
            path: json_path.to_string(),
            eq: eq.to_string(),
        });
        self
    }

    /// Replace all matchers with input
    pub fn with_matchers(mut self, matchers: Vec<Matcher>) -> Self {
        self.matchers = matchers;
        self
    }
}
#[derive(Default)]
pub struct DeceitResponseBuilder {
    code: Option<u16>,

    matchers: Vec<Matcher>,

    headers: Vec<(String, String)>,

    processors: Vec<Processor>,

    output_type: OutputType,

    output: String,
}

impl DeceitResponseBuilder {
    pub fn build(self) -> DeceitResponse {
        DeceitResponse {
            code: self.code,
            matchers: self.matchers,
            headers: self.headers,
            processors: self.processors,
            output_type: self.output_type,
            output: self.output,
        }
    }

    pub fn code(mut self, code: u16) -> Self {
        self.code = Some(code);
        self
    }

    /// Add response header for this response
    pub fn add_header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn add_processor(mut self, processor: Processor) -> Self {
        self.processors.push(processor);
        self
    }

    pub fn with_output(mut self, output: &str) -> Self {
        self.output = output.to_string();
        self
    }

    pub fn with_output_type(mut self, output_type: OutputType) -> Self {
        self.output_type = output_type;
        self
    }

    //
    // Matchers configuration
    //
    pub fn add_matcher(mut self, matcher: Matcher) -> Self {
        self.matchers.push(matcher);
        self
    }

    pub fn require_method(mut self, http_method: &str) -> Self {
        self.matchers.push(Matcher::Method {
            eq: http_method.to_string(),
        });
        self
    }

    pub fn require_header(mut self, key: &str, value: &str) -> Self {
        self.matchers.push(Matcher::Header {
            key: key.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn require_query_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::QueryArg {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn require_path_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::PathArg {
            name: name.to_string(),
            value: value.to_string(),
        });
        self
    }

    pub fn require_json_match(mut self, json_path: &str, eq: &str) -> Self {
        self.matchers.push(Matcher::Json {
            path: json_path.to_string(),
            eq: eq.to_string(),
        });
        self
    }

    /// Replace all matchers with input
    pub fn with_matchers(mut self, matchers: Vec<Matcher>) -> Self {
        self.matchers = matchers;
        self
    }
}
