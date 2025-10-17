use jsonpath_rust::JsonPath as _;
use serde::{Deserialize, Serialize};

use crate::RequestContext;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Matcher {
    Method {
        eq: String,
    },
    Header {
        key: String,
        value: String,
    },
    /// Treats request payload as JSON.
    ///  - `path` JSON Path expression to extract value
    ///  - `eq` value to match agains extracted from JSON Path
    Json {
        path: String,
        eq: String,
    },
}

pub fn is_matcher_approves(matcher: &Matcher, ctx: &RequestContext) -> bool {
    match matcher {
        Matcher::Method { eq } => match_method(eq.as_str(), ctx),
        Matcher::Header { key, value } => match_header(key.as_str(), value.as_str(), ctx),
        Matcher::Json { path, eq } => match_json(path.as_str(), eq.as_str(), ctx),
    }
}

pub fn match_method(method: &str, ctx: &RequestContext) -> bool {
    method
        .to_uppercase()
        .contains(&ctx.req.method().to_string())
}

pub fn match_header(key: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(header_value) = ctx.req.headers().get(key) else {
        return false;
    };
    header_value.to_str().map_or(false, |v| v == value)
}

pub fn match_json(path: &str, value: &str, ctx: &RequestContext) -> bool {
    let body = String::from_utf8_lossy(&ctx.body);

    let json = match serde_json::from_slice::<serde_json::Value>(body.as_bytes()) {
        Ok(json) => json,
        Err(e) => {
            log::error!("Can't parse request as JSON {e}");
            return false;
        }
    };

    json.query_with_path(path).map_or(false, |result| {
        if result.len() == 1 {
            result[0]
                .clone()
                .val()
                .as_str()
                .map_or(false, |val| val == value)
        } else {
            false
        }
    })
}
