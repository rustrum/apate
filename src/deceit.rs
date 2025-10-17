use std::collections::HashMap;

use actix_web::{HttpResponse, body::BoxBody, http::StatusCode};
use anyhow::bail;
use rand::{Rng, RngCore as _};
use serde::{Deserialize, Serialize};

use crate::{
    RequestContext,
    matchers::{Matcher, is_matcher_approves},
};

/// Unit responsible for mocking actual URIs
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Deceit {
    pub uris: Vec<String>,

    /// Default response code in this deceit.
    #[serde(default)]
    pub code: u16,

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

/// TODO add query sring map
#[derive(Serialize)]
pub struct ResponseContext<'a> {
    path: &'a str,

    headers: HashMap<&'a str, &'a str>,

    input_json: Option<serde_json::Value>,
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
            input_json,
        };

        let result = self.render(&rctx)?;

        let response_code = if let Some(code) = self.code {
            code
        } else {
            deceit.code
        };

        let response: HttpResponse =
            HttpResponse::with_body(StatusCode::from_u16(response_code)?, BoxBody::new(result));

        Ok(response)
    }

    pub fn render(&self, ctx: &ResponseContext) -> anyhow::Result<String> {
        match self.content_type {
            BodyContent::Jinja => {
                let mut env = minijinja::Environment::new();
                env.add_template("response.jinja", &self.content)?;

                add_clean_tpl_functions(&mut env);

                let tpl = env.get_template("response.jinja")?;
                let response = tpl.render(ctx)?;

                Ok(response)
            }
        }
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
