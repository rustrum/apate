use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
};

use actix_web::{HttpResponse, HttpResponseBuilder, http::StatusCode};
use color_eyre::eyre::bail;
use serde::{Deserialize, Serialize};

use crate::{
    ApateCounters, RequestContext,
    matchers::{Matcher, is_matcher_approves},
    output::OutputType,
    test::AppConfig,
};

const DEFAULT_RESPONSE_CODE: u16 = 200;

/// Unit responsible for mocking actual URIs
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Deceit {
    pub uris: Vec<String>,

    /// Common headers for all responses in this deceit.
    #[serde(default)]
    pub headers: Vec<(String, String)>,

    #[serde(default)]
    pub matchers: Vec<Matcher>,

    /// Will parse request as JSON
    /// Could be pre processor as well
    #[serde(default)]
    pub json_request: bool,

    #[serde(default)]
    pub processors: Vec<Processor>,

    pub responses: Vec<DeceitResponse>,
}

impl Deceit {
    pub fn has_match(&self, ctx: &RequestContext) -> Option<usize> {
        // Top level matchers
        for matcher in &self.matchers {
            if !is_matcher_approves(matcher, ctx) {
                return None;
            }
        }

        for (idx, dr) in self.responses.iter().enumerate() {
            if dr.matchers.is_empty() {
                // Empty matchers - always yes
                return Some(idx);
            }
            for matcher in &dr.matchers {
                if is_matcher_approves(matcher, ctx) {
                    return Some(idx);
                }
            }
        }

        None
    }

    pub fn process_response(
        &self,
        idx: usize,
        ctx: &RequestContext,
        cnt: &ApateCounters,
    ) -> color_eyre::Result<HttpResponse> {
        for matcher in &self.matchers {
            if !is_matcher_approves(matcher, ctx) {
                bail!("Top level matcher does not approve this action {matcher:?}");
            }
        }

        let Some(d) = self.responses.get(idx) else {
            bail!("Wow we definitely must have response for this index {idx}");
        };

        d.process(self, ctx, cnt)
    }
}

/// Context for response renderer and pre/post processors as well.
#[derive(Serialize)]
pub struct ResponseContext<'a> {
    path: &'a str,

    headers: HashMap<&'a str, &'a str>,

    query_args: &'a HashMap<String, String>,

    path_args: &'a HashMap<&'a str, &'a str>,

    request_json: Option<serde_json::Value>,

    #[serde(skip_serializing)]
    pub(crate) response_code: Arc<AtomicU16>,

    #[serde(skip_serializing)]
    pub(crate) counters: &'a ApateCounters,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeceitResponse {
    /// Code for this particular response
    #[serde(default)]
    pub code: Option<u16>,

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
    pub fn process(
        &self,
        deceit: &Deceit,
        ctx: &RequestContext,
        cnt: &ApateCounters,
    ) -> color_eyre::Result<HttpResponse> {
        for matcher in &self.matchers {
            if !is_matcher_approves(matcher, ctx) {
                bail!("Top level matcher does not approve this action {matcher:?}");
            }
        }

        let mut headers = HashMap::new();
        for (k, v) in ctx.req.headers().iter() {
            headers.insert(k.as_str(), v.to_str()?);
        }

        let request_json = if deceit.json_request && !ctx.body.trim_ascii().is_empty() {
            let body = String::from_utf8_lossy(ctx.body);
            Some(serde_json::from_slice::<serde_json::Value>(
                body.as_bytes(),
            )?)
        } else {
            None
        };

        let rctx = ResponseContext {
            path: ctx.req.path(),
            headers,
            query_args: ctx.args_query,
            path_args: ctx.args_path,
            request_json,
            response_code: Arc::new(AtomicU16::new(0)),
            counters: cnt,
        };

        let output_body =
            crate::output::build_response_body(self.output_type, &self.output, &rctx)?;

        let response_code_from_tpl = rctx.response_code.load(Ordering::Relaxed);
        let response_code = if response_code_from_tpl > 0 {
            response_code_from_tpl
        } else {
            self.code.unwrap_or(DEFAULT_RESPONSE_CODE)
        };

        let mut rbuilder: HttpResponseBuilder =
            HttpResponseBuilder::new(StatusCode::from_u16(response_code)?);
        insert_response_headers(&mut rbuilder, &deceit.headers, &self.headers);

        Ok(rbuilder.body(output_body))
    }
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

/// Set up custom logic that could be executed before/after rendering response.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Processor {
    Lua {
        scope: ProcessorScope,
        script: String,
    },
    LuaFile {
        scope: ProcessorScope,
        path: String,
    },
    /// Reserved for custom user processors
    Custom {
        scope: ProcessorScope,
        id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ProcessorScope {
    Pre,
    Post,
}

pub struct DeceitBuilder {
    uris: Vec<String>,

    headers: Vec<(String, String)>,

    matchers: Vec<Matcher>,

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
        }
    }

    pub fn build(self) -> Deceit {
        Deceit {
            uris: self.uris,
            headers: self.headers,
            matchers: self.matchers,
            json_request: self.json_request,
            processors: vec![],
            responses: self.responses,
        }
    }

    /// Wraps single [`Deceit`] into a [`AppConfig`] with default parameters.
    pub fn to_app_config(self) -> AppConfig {
        AppConfig {
            specs: crate::ApateSpecs {
                deceit: vec![self.build()],
            },
            ..Default::default()
        }
    }

    /// Wraps single [`Deceit`] into a [`AppConfig`] with specified port.
    pub fn to_app_config_with_port(self, port: u16) -> AppConfig {
        AppConfig {
            port,
            specs: crate::ApateSpecs {
                deceit: vec![self.build()],
            },
        }
    }

    pub fn add_header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
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

    output_type: OutputType,

    output: String,
}

impl DeceitResponseBuilder {
    pub fn build(self) -> DeceitResponse {
        DeceitResponse {
            code: self.code,
            matchers: self.matchers,
            headers: self.headers,
            processors: vec![],
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
