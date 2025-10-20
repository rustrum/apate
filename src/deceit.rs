use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
};

use actix_web::{HttpResponse, HttpResponseBuilder, http::StatusCode};
use anyhow::bail;
use rand::{Rng, RngCore as _};
use serde::{Deserialize, Serialize};

use crate::{
    RequestContext,
    matchers::{Matcher, is_matcher_approves},
};

const DEFAULT_RESPONSE_CODE: u16 = 200;

/// Unit responsible for mocking actual URIs
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    ) -> anyhow::Result<HttpResponse> {
        for matcher in &self.matchers {
            if !is_matcher_approves(matcher, ctx) {
                bail!("Top level matcher does not approve this action {matcher:?}");
            }
        }

        let Some(d) = self.responses.get(idx) else {
            bail!("Wow we definitely must have response for this index {idx}");
        };

        d.process(self, ctx)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BodyContent {
    Jinja,
}

impl Default for BodyContent {
    fn default() -> Self {
        BodyContent::Jinja
    }
}

/// Context for response renderer and pre/post processors as well.
#[derive(Serialize)]
pub struct ResponseContext<'a> {
    path: &'a str,

    headers: HashMap<&'a str, &'a str>,

    query_args: &'a HashMap<String, String>,

    path_args: &'a HashMap<&'a str, &'a str>,

    input_json: Option<serde_json::Value>,

    #[serde(skip_serializing)]
    response_code: Arc<AtomicU16>,
}

///
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeceitResponse {
    /// Code for this particular response
    #[serde(default)]
    pub code: Option<u16>,

    #[serde(default)]
    pub matchers: Vec<Matcher>,

    #[serde(default)]
    headers: Vec<(String, String)>,

    #[serde(default)]
    pub processors: Vec<Processor>,

    #[serde(default)]
    pub content_type: BodyContent,

    #[serde(default)]
    pub content: String,
}

impl DeceitResponse {
    pub fn process(&self, deceit: &Deceit, ctx: &RequestContext) -> anyhow::Result<HttpResponse> {
        for matcher in &self.matchers {
            if !is_matcher_approves(matcher, ctx) {
                bail!("Top level matcher does not approve this action {matcher:?}");
            }
        }

        let mut headers = HashMap::new();
        for (k, v) in ctx.req.headers().iter() {
            headers.insert(k.as_str(), v.to_str()?);
        }

        let input_json = if deceit.json_request {
            let body = String::from_utf8_lossy(&ctx.body);
            Some(serde_json::from_slice::<serde_json::Value>(
                body.as_bytes(),
            )?)
        } else {
            None
        };

        let rctx = ResponseContext {
            path: ctx.req.path(),
            headers,
            query_args: &ctx.args_query,
            path_args: &ctx.args_path,
            input_json,
            response_code: Arc::new(AtomicU16::new(0)),
        };

        let result = self.render(&rctx)?;

        let response_code_from_tpl = rctx.response_code.load(Ordering::Relaxed);
        let response_code = if response_code_from_tpl > 0 {
            response_code_from_tpl
        } else {
            self.code.unwrap_or(DEFAULT_RESPONSE_CODE)
        };

        let mut rbuilder: HttpResponseBuilder =
            HttpResponseBuilder::new(StatusCode::from_u16(response_code)?);
        insert_response_headers(&mut rbuilder, &deceit.headers, &self.headers);

        Ok(rbuilder.body(result))
    }

    pub fn render(&self, ctx: &ResponseContext) -> anyhow::Result<String> {
        match self.content_type {
            BodyContent::Jinja => {
                let mut env = minijinja::Environment::new();
                env.add_template("response.jinja", &self.content)?;
                // env.set_trim_blocks(true);
                // env.set_lstrip_blocks(true);
                // env.set_keep_trailing_newline(false);

                add_clean_tpl_functions(&mut env);

                // let mut env = env.clone();

                let atomic_val = ctx.response_code.clone();

                env.add_function("response_code", move |code: u16| {
                    atomic_val.store(code, Ordering::Relaxed);
                });

                let tpl = env.get_template("response.jinja")?;
                let response = tpl.render(ctx)?;

                Ok(response)
            }
        }
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
        scope: ProcessorRunScope,
        script: String,
    },
    LuaFile {
        scope: ProcessorRunScope,
        path: String,
    },
    /// Reserved for custom user processors
    Custom {
        scope: ProcessorRunScope,
        id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ProcessorRunScope {
    Pre,
    Post,
}

fn add_clean_tpl_functions(env: &mut minijinja::Environment) {
    env.add_function("random_num", |a: Option<u128>, b: Option<u128>| {
        let Some(first) = a else {
            return rand::random::<u128>().to_string();
        };

        let Some(second) = b else {
            let num = rand::random::<u128>() % first;
            return num.to_string();
        };

        rand::rng().random_range(first..second).to_string()
    });

    env.add_function("random_hex", |length: Option<u64>| {
        let bytesn = if let Some(n) = length {
            (n / 2) as usize
        } else {
            64
        };

        let mut bytes = Vec::<u8>::with_capacity(bytesn);
        rand::rng().fill_bytes(&mut bytes);

        hex::encode(bytes)
    });
}
