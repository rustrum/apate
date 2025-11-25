//! Matchers are just specific checks to run against input HTTP request.
//! In order to proceed forward all matches must return true.
//!
//! Matchers behave different on deceit ane response levels:
//!
//!  - if matchers failed on deceit level, than next deceit will be handled
//!  - if matchers failed on response level then next response will be handled
//!  - if all matchers responses failed, than next deceit will be handled

use jsonpath_rust::JsonPath as _;
use serde::{Deserialize, Serialize};

use crate::RequestContext;

/// Matchers process request data and return boolean result that affects [`crate::deceit::Deceit`] processing behavior.
/// To process response all matchers on deceit level and appropriate response should pass.
/// .
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Matcher {
    /// HTTP request method matcher
    Method {
        eq: String,
    },
    /// HTTP request header matcher
    Header {
        key: String,
        value: String,
    },
    /// Matches query string arguments
    QueryArg {
        name: String,
        value: String,
    },
    /// Matching URI path arguments extracted using paths patterns like `/user/:user_id` etc.
    PathArg {
        name: String,
        value: String,
    },
    /// Run match logic against request payload as JSON.
    /// NOTICE you must enable request JSON parsing for [`crate::deceit::Deceit`].
    ///
    ///  - `path` JSON Path expression to extract value
    ///  - `eq` value to match against one extracted from JSON Path
    Json {
        path: String,
        eq: String,
    },

    Lua {
        script: String,
    },
}

pub fn is_matcher_approves(matcher: &Matcher, ctx: &RequestContext) -> bool {
    match matcher {
        Matcher::QueryArg { name, value } => match_query_arg(name.as_str(), value.as_str(), ctx),
        Matcher::PathArg { name, value } => match_path_arg(name.as_str(), value.as_str(), ctx),
        Matcher::Method { eq } => match_method(eq.as_str(), ctx),
        Matcher::Header { key, value } => match_header(key.as_str(), value.as_str(), ctx),
        Matcher::Json { path, eq } => match_json(path.as_str(), eq.as_str(), ctx),
        Matcher::Lua { script } => lua::match_lua(script.as_str(), ctx),
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

mod lua {
    use std::collections::HashMap;

    use actix_web::http::header::HeaderMap;
    use mlua::prelude::*;

    use crate::RequestContext;

    /// Ideally it should not copy all data from RequestContext but reuse it instead.
    /// Current implementation of a RequestContext with lifetimes does not allow to do it.
    struct LuaRequestContext {
        path: String,
        args_query: HashMap<String, String>,
        args_path: HashMap<String, String>,
        headers: HeaderMap,
    }

    impl LuaRequestContext {
        fn new(ctx: &RequestContext) -> Self {
            Self {
                path: ctx.path.as_str().to_string(),
                headers: ctx.req.headers().clone(),
                args_query: ctx.args_query.clone(),
                args_path: ctx
                    .args_path
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            }
        }
    }

    impl LuaUserData for LuaRequestContext {
        fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
            methods.add_method("path", |_, this, ()| Ok(this.path.clone()));

            methods.add_method("get_query_arg", |_, this, key: String| {
                Ok(this.args_query.get(&key).cloned())
            });

            methods.add_method("get_path_arg", |_, this, key: String| {
                Ok(this.args_path.get(&key).cloned())
            });

            methods.add_method("get_header", |_, this, key: String| {
                let result = if let Some(header) = this.headers.get(&key) {
                    let mapped = header.to_str().map_err(mlua::Error::external)?.to_string();
                    Some(mapped)
                } else {
                    None
                };
                Ok(result)
            });
        }
    }

    pub fn match_lua(script: &str, ctx: &RequestContext) -> bool {
        let lua = Lua::new();

        lua.globals()
            .set(
                "log",
                lua.create_function(|_, (msg,): (String,)| {
                    log::info!("LUA: {msg}");
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        lua.globals()
            .set(
                "warn",
                lua.create_function(|_, (msg,): (String,)| {
                    log::warn!("LUA: {msg}");
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        let lua_fn = lua.load(script).into_function();

        let lua_fn = match lua_fn {
            Ok(v) => v,
            Err(e) => {
                log::error!("Can't compile LUA script {e:?}");
                return false;
            }
        };

        let result = lua_fn.call::<mlua::Value>((LuaRequestContext::new(ctx),));

        match result {
            Ok(v) => {
                if let Some(r) = v.as_boolean() {
                    r
                } else {
                    log::error!("Can't parse LUA matchers result as boolean");
                    false
                }
            }
            Err(e) => {
                log::error!("Can't execute LUA matcher {e:?}");
                false
            }
        }
    }
}
