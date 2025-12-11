//! Deceit is the unit responsible for processing serveral status URIs or path patters.
//! All deceit related logic is placed into this module.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicU16},
};

use actix_router::{Path, ResourceDef};
use actix_web::{http::StatusCode, web::Bytes};

use serde::{Deserialize, Serialize};

use crate::{
    ApateConfig, ApateCounters, RequestContext, ResourceRef,
    matchers::{Matcher, matchers_and},
    output::OutputType,
    processors::Processor,
    rhai::RhaiState,
};

pub const DEFAULT_RESPONSE_CODE: StatusCode = StatusCode::OK;

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
        rhai: &RhaiState,
    ) -> Option<usize> {
        if !matchers_and(rref, rhai, ctx, &self.matchers) {
            return None;
        }

        // Deceit level matchers
        for (idx, dr) in self.responses.iter().enumerate() {
            if dr.matchers.is_empty() {
                // Empty matchers - always yes
                return Some(idx);
            }
            let deceit_ref = rref.with_level(idx);

            if matchers_and(&deceit_ref, rhai, ctx, &dr.matchers) {
                return Some(idx);
            }
        }

        None
    }
}

/// Context for output renderers and pre/post processors as well.
#[derive(Clone)]
pub struct DeceitResponseContext {
    pub method: String,

    pub path: Arc<String>,

    pub headers: Arc<HashMap<String, String>>,

    pub query_args: Arc<HashMap<String, String>>,

    pub path_args: Arc<HashMap<String, String>>,

    pub response_code: Arc<AtomicU16>,

    pub counters: ApateCounters,

    pub request_body: Arc<Bytes>,

    #[allow(clippy::type_complexity)]
    pub request_json: Arc<Mutex<Option<Result<Arc<serde_json::Value>, String>>>>,
}

impl DeceitResponseContext {
    pub fn load_request_json(&self) -> Result<Arc<serde_json::Value>, String> {
        let mut guard = self
            .request_json
            .lock()
            .expect("WTF stuff. No multithread access here expected.");

        if let Some(value) = (*guard).as_ref() {
            return value.clone();
        }

        let body = String::from_utf8_lossy(&self.request_body);
        if body.trim().is_empty() {
            return Ok(Arc::new(serde_json::Value::Null));
        }
        let json_value = serde_json::from_slice::<serde_json::Value>(body.as_bytes())
            .map(Arc::new)
            .map_err(|e| format!("{e}"));

        *guard = Some(json_value.clone());

        json_value
    }
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

    #[serde(default, rename = "type")]
    pub output_type: OutputType,

    #[serde(default)]
    pub output: String,
}

pub fn create_response_context(
    ctx: RequestContext,
    cnt: ApateCounters,
) -> color_eyre::Result<DeceitResponseContext> {
    Ok(DeceitResponseContext {
        method: ctx.method.clone(),
        path: ctx.request_path.clone(),
        headers: ctx.headers.clone(),
        query_args: ctx.query_args.clone(),
        path_args: ctx.path_args.clone(),
        request_json: Default::default(),
        response_code: Arc::new(AtomicU16::new(0)),
        counters: cnt,
        request_body: ctx.body.clone(),
    })
}

pub struct DeceitBuilder {
    uris: Vec<String>,

    headers: Vec<(String, String)>,

    matchers: Vec<Matcher>,

    processors: Vec<Processor>,

    responses: Vec<DeceitResponse>,
}

impl DeceitBuilder {
    pub fn with_uris<T: AsRef<str>>(uris: &[T]) -> Self {
        let uris = uris.iter().map(|u| u.as_ref().to_string()).collect();
        Self {
            uris,
            headers: Vec::new(),
            matchers: Vec::new(),
            responses: Vec::new(),
            processors: Vec::new(),
        }
    }

    pub fn build(self) -> Deceit {
        Deceit {
            uris: self.uris,
            headers: self.headers,
            matchers: self.matchers,
            processors: self.processors,
            responses: self.responses,
        }
    }

    /// Wraps single [`Deceit`] into a [`ApateConfig`] with default parameters.
    pub fn to_app_config(self) -> ApateConfig {
        ApateConfig {
            specs: crate::ApateSpecs {
                deceit: vec![self.build()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Wraps single [`Deceit`] into a [`ApateConfig`] with specified port.
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
            negate: false,
        });
        self
    }

    pub fn require_header(mut self, key: &str, value: &str) -> Self {
        self.matchers.push(Matcher::Header {
            key: key.to_string(),
            value: value.to_string(),
            negate: false,
        });
        self
    }

    pub fn require_query_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::QueryArg {
            name: name.to_string(),
            value: value.to_string(),
            negate: false,
        });
        self
    }

    pub fn require_path_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::PathArg {
            name: name.to_string(),
            value: value.to_string(),
            negate: false,
        });
        self
    }

    pub fn require_json_match(mut self, json_path: &str, eq: &str) -> Self {
        self.matchers.push(Matcher::Json {
            path: json_path.to_string(),
            eq: eq.to_string(),
            negate: false,
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
            negate: false,
        });
        self
    }

    pub fn require_header(mut self, key: &str, value: &str) -> Self {
        self.matchers.push(Matcher::Header {
            key: key.to_string(),
            value: value.to_string(),
            negate: false,
        });
        self
    }

    pub fn require_query_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::QueryArg {
            name: name.to_string(),
            value: value.to_string(),
            negate: false,
        });
        self
    }

    pub fn require_path_arg(mut self, name: &str, value: &str) -> Self {
        self.matchers.push(Matcher::PathArg {
            name: name.to_string(),
            value: value.to_string(),
            negate: false,
        });
        self
    }

    pub fn require_json_match(mut self, json_path: &str, eq: &str) -> Self {
        self.matchers.push(Matcher::Json {
            path: json_path.to_string(),
            eq: eq.to_string(),
            negate: false,
        });
        self
    }

    /// Replace all matchers with input
    pub fn with_matchers(mut self, matchers: Vec<Matcher>) -> Self {
        self.matchers = matchers;
        self
    }
}
