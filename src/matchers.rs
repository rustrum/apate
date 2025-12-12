//! Matchers are just specific checks to run against input HTTP request.
//! In order to proceed forward all matches must return true.
//!
//! Matchers behave different on deceit ane response levels:
//!
//!  - if matchers failed on deceit level, than next deceit will be handled
//!  - if matchers failed on response level then next response will be handled
//!  - if all matchers responses failed, than next deceit will be handled
use std::fmt::Display;

use jsonpath_rust::JsonPath as _;
use rhai::{AST, Array, Engine, Scope};
use serde::{Deserialize, Serialize};

use crate::{
    RequestContext, ResourceRef,
    rhai::{RhaiRequestContext, RhaiState},
};

/// Matchers process request data and return boolean result that affects [`crate::deceit::Deceit`] processing behavior.
/// To process response all matchers on deceit level and appropriate response should pass.
/// .
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Matcher {
    And {
        matchers: Vec<Matcher>,
    },
    Or {
        matchers: Vec<Matcher>,
    },
    /// HTTP request method matcher
    Method {
        eq: String,
        #[serde(default)]
        negate: bool,
    },
    /// HTTP request header matcher
    Header {
        key: String,
        value: String,
        #[serde(default)]
        negate: bool,
    },
    /// Matches query string arguments
    QueryArg {
        name: String,
        value: String,
        #[serde(default)]
        negate: bool,
    },
    /// Matching URI path arguments extracted using paths patterns like `/user/:user_id` etc.
    PathArg {
        name: String,
        value: String,
        #[serde(default)]
        negate: bool,
    },
    /// Run match logic against request payload as JSON.
    /// NOTICE you must enable request JSON parsing for [`crate::deceit::Deceit`].
    ///
    ///  - `path` JSON Path expression to extract value
    ///  - `eq` value to match against one extracted from JSON Path
    Json {
        path: String,
        eq: String,
        #[serde(default)]
        negate: bool,
    },
    Rhai {
        script: String,
    },

    RhaiRef {
        id: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl Display for Matcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::And { .. } => "AND",
            Self::Or { .. } => "OR",
            Self::Method { .. } => "METHOD",
            Self::Header { .. } => "HEADER",
            Self::PathArg { .. } => "PATH_ARG",
            Self::QueryArg { .. } => "QUERY_ARG",
            Self::Json { .. } => "JSON",
            Self::Rhai { .. } => "RHAI",
            Self::RhaiRef { .. } => "RHAI_REF",
        };

        write!(f, "{}", value)
    }
}

pub fn matchers_and(
    rref: &ResourceRef,
    rhai: &RhaiState,
    ctx: &RequestContext,
    matchers: &[Matcher],
) -> bool {
    for (mid, matcher) in matchers.iter().enumerate() {
        let matcher_ref = rref.with_level(mid);
        if !is_matcher_approves(&matcher_ref, rhai, ctx, matcher) {
            return false;
        }
    }
    true
}

pub fn matchers_or(
    rref: &ResourceRef,
    rhai: &RhaiState,
    ctx: &RequestContext,
    matchers: &[Matcher],
) -> bool {
    log::debug!("Matcher OR started");
    for (mid, matcher) in matchers.iter().enumerate() {
        let matcher_ref = rref.with_level(mid);
        if is_matcher_approves(&matcher_ref, rhai, ctx, matcher) {
            log::debug!("Matcher OR ok");
            return true;
        }
    }
    false
}

pub fn is_matcher_approves(
    rref: &ResourceRef,
    rhai: &RhaiState,
    ctx: &RequestContext,
    matcher: &Matcher,
) -> bool {
    let result = match matcher {
        Matcher::QueryArg {
            name,
            value,
            negate,
        } => flip_boolean(match_query_arg(name.as_str(), value.as_str(), ctx), *negate),
        Matcher::PathArg {
            name,
            value,
            negate,
        } => flip_boolean(match_path_arg(name.as_str(), value.as_str(), ctx), *negate),
        Matcher::Method { eq, negate } => flip_boolean(match_method(eq.as_str(), ctx), *negate),
        Matcher::Header { key, value, negate } => {
            flip_boolean(match_header(key.as_str(), value.as_str(), ctx), *negate)
        }
        Matcher::Json { path, eq, negate } => {
            flip_boolean(match_json(path.as_str(), eq.as_str(), ctx), *negate)
        }
        Matcher::Rhai { script } => match_rhai(rhai, rref, script, ctx),
        Matcher::RhaiRef { id, args } => match_rhai_ref(rhai, rref, id.as_str(), ctx, args.clone()),
        Matcher::And { matchers } => matchers_and(rref, rhai, ctx, matchers),
        Matcher::Or { matchers } => matchers_or(rref, rhai, ctx, matchers),
    };

    log::trace!("Matcher {matcher} id:{rref} result:{result}");
    result
}

#[inline(always)]
fn flip_boolean(value: bool, negate: bool) -> bool {
    if negate { !value } else { value }
}

pub fn match_path_arg(name: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(qvalue) = ctx.path_args.get(name) else {
        return false;
    };
    value == *qvalue
}

pub fn match_query_arg(name: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(qvalue) = ctx.query_args.get(name) else {
        return false;
    };
    value == qvalue.as_str()
}

pub fn match_method(method: &str, ctx: &RequestContext) -> bool {
    method.to_uppercase().contains(&ctx.method)
}

pub fn match_header(key: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(header_value) = ctx.headers.get(key) else {
        return false;
    };
    header_value.as_str() == value
}

pub fn match_json(path: &str, value: &str, ctx: &RequestContext) -> bool {
    let json = match ctx.load_body_as_json() {
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

pub fn match_rhai_ref(
    rhai: &RhaiState,
    rref: &ResourceRef,
    script_id: &str,
    ctx: &RequestContext,
    args: Vec<String>,
) -> bool {
    let (engine, ast) = match rhai.get_exec_global(script_id) {
        Ok(lfn) => lfn,
        Err(e) => {
            log::error!("Can't load Rhai top level scrip by id:{script_id} path:{rref} {e:?}");
            return false;
        }
    };

    let args = args.into_iter().map(Into::into).collect();
    call_rhai(&engine, &ast, ctx.clone().into(), args)
}

pub fn match_rhai(
    rhai: &RhaiState,
    rref: &ResourceRef,
    script: &str,
    ctx: &RequestContext,
) -> bool {
    let id = rref.to_resource_id("lua-matcher");

    let (engine, ast) = match rhai.get_exec(id.clone(), script) {
        Ok(a) => a,
        Err(e) => {
            log::error!("Can't load Rhai matcher by path:{rref} {e:?}");
            return false;
        }
    };
    call_rhai(&engine, &ast, ctx.clone().into(), Array::new())
}

fn call_rhai(engine: &Engine, ast: &AST, ctx: RhaiRequestContext, args: Array) -> bool {
    let mut scope = Scope::new();
    scope.set_value("ctx", ctx);
    scope.set_value("args", args);

    match engine.eval_ast_with_scope::<bool>(&mut scope, ast) {
        Ok(v) => v,
        Err(e) => {
            log::error!("Can't execute Rhai matcher {e:?}");
            false
        }
    }
}
