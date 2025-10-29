use jsonpath_rust::JsonPath as _;
use serde::{Deserialize, Serialize};

use crate::RequestContext;

/// Matchers process request data and return boolean result that affetcts [`crate::deceit::Deceit`] selecting behaviour.
/// Matchers defined on a top level (Deceit) must all path in order to start processing responses.
/// Response level matches determine which response should be processed (basically first one that return all true).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Matcher {
    /// HTTP request method matcher
    Method { eq: String },
    /// HTTP request header matcher
    Header { key: String, value: String },
    /// Matches query string arguments
    QueryArg { name: String, value: String },
    /// Matching URI path arguments extracted using paths patterns like `/user/:user_id` etc.
    PathArg { name: String, value: String },
    /// Run match logic agains request payload as JSON.
    ///  - `path` JSON Path expression to extract value
    ///  - `eq` value to match agains one extracted from JSON Path
    Json { path: String, eq: String },
}

pub fn is_matcher_approves(matcher: &Matcher, ctx: &RequestContext) -> bool {
    match matcher {
        Matcher::QueryArg { name, value } => match_query_arg(name.as_str(), value.as_str(), ctx),
        Matcher::PathArg { name, value } => match_path_arg(name.as_str(), value.as_str(), ctx),
        Matcher::Method { eq } => match_method(eq.as_str(), ctx),
        Matcher::Header { key, value } => match_header(key.as_str(), value.as_str(), ctx),
        Matcher::Json { path, eq } => match_json(path.as_str(), eq.as_str(), ctx),
    }
}

pub fn match_path_arg(name: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(qvalue) = ctx.args_path.get(name) else {
        return false;
    };
    value == *qvalue
}

pub fn match_query_arg(name: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(qvalue) = ctx.args_query.get(name) else {
        return false;
    };
    value == qvalue.as_str()
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
    header_value.to_str().is_ok_and(|v| v == value)
}

pub fn match_json(path: &str, value: &str, ctx: &RequestContext) -> bool {
    let body = String::from_utf8_lossy(ctx.body);

    let json = match serde_json::from_slice::<serde_json::Value>(body.as_bytes()) {
        Ok(json) => json,
        Err(e) => {
            log::error!("Can't parse request as JSON {e}");
            return false;
        }
    };

    json.query_with_path(path).is_ok_and(|result| {
        if result.len() == 1 {
            result[0].clone().val().as_str() == Some(value)
        } else {
            false
        }
    })
}
