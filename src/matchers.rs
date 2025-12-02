//! Matchers are just specific checks to run against input HTTP request.
//! In order to proceed forward all matches must return true.
//!
//! Matchers behave different on deceit ane response levels:
//!
//!  - if matchers failed on deceit level, than next deceit will be handled
//!  - if matchers failed on response level then next response will be handled
//!  - if all matchers responses failed, than next deceit will be handled
use jsonpath_rust::JsonPath as _;
use mlua::Function;
use rhai::{AST, Array, Engine, Scope};
use serde::{Deserialize, Serialize};

use crate::{
    RequestContext, ResourceRef,
    lua::{LuaRequestContext, LuaState},
    rhai::{RhaiRequestContext, RhaiState},
};

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

    LuaScript {
        id: String,
        args: Vec<String>,
    },

    Rhai {
        script: String,
    },

    RhaiScript {
        id: String,
        args: Vec<String>,
    },
}

pub fn is_matcher_approves(
    rref: &ResourceRef,
    matcher: &Matcher,
    ctx: &RequestContext,
    lua: &LuaState,
    rhai: &RhaiState,
) -> bool {
    match matcher {
        Matcher::QueryArg { name, value } => match_query_arg(name.as_str(), value.as_str(), ctx),
        Matcher::PathArg { name, value } => match_path_arg(name.as_str(), value.as_str(), ctx),
        Matcher::Method { eq } => match_method(eq.as_str(), ctx),
        Matcher::Header { key, value } => match_header(key.as_str(), value.as_str(), ctx),
        Matcher::Json { path, eq } => match_json(path.as_str(), eq.as_str(), ctx),
        Matcher::Lua { script } => match_lua(lua, rref, script.as_str(), ctx.clone()),
        Matcher::LuaScript { id, args } => {
            match_lua_script(lua, rref, id.as_str(), ctx.clone(), args.clone())
        }
        Matcher::Rhai { script } => match_rhai(rhai, rref, script, ctx),
        Matcher::RhaiScript { id, args } => {
            match_rhai_script(rhai, rref, id.as_str(), ctx, args.clone())
        }
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
    method.to_uppercase().contains(&ctx.method)
}

pub fn match_header(key: &str, value: &str, ctx: &RequestContext) -> bool {
    let Some(header_value) = ctx.headers.get(key) else {
        return false;
    };
    header_value.as_str() == value
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

    json.query_with_path(path).is_ok_and(|result| {
        if result.len() == 1 {
            result[0].clone().val().as_str() == Some(value)
        } else {
            false
        }
    })
}

pub fn match_lua_script(
    lua: &LuaState,
    rref: &ResourceRef,
    script_id: &str,
    ctx: RequestContext,
    args: Vec<String>,
) -> bool {
    let lua_fn = match lua.get_lua_script(script_id) {
        Ok(lfn) => lfn,
        Err(e) => {
            log::error!(
                "Can't load LUA top level scrip by id:{script_id} path:{} {e:?}",
                rref.as_string()
            );
            return false;
        }
    };
    call_lua_fn(lua_fn, ctx.into(), args)
}

pub fn match_lua(lua: &LuaState, rref: &ResourceRef, script: &str, ctx: RequestContext) -> bool {
    let id = rref.to_resource_id("lua-matcher");
    let lua_fn = match lua.to_lua_function(id.clone(), script) {
        Ok(lfn) => lfn,
        Err(e) => {
            log::error!("Can't load LUA matcher by path:{} {e:?}", rref.as_string());
            return false;
        }
    };
    call_lua_fn(lua_fn, ctx.into(), vec![])
}

fn call_lua_fn(lua_fn: Function, ctx: LuaRequestContext, args: Vec<String>) -> bool {
    let result = lua_fn.call::<mlua::Value>((ctx, args));

    match result {
        Ok(v) => {
            if let Some(r) = v.as_boolean() {
                r
            } else {
                log::error!("Can't parse LUA matchers result as boolean. {v:?}");
                false
            }
        }
        Err(e) => {
            log::error!("Can't execute LUA matcher {e:?}");
            false
        }
    }
}

pub fn match_rhai_script(
    rhai: &RhaiState,
    rref: &ResourceRef,
    script_id: &str,
    ctx: &RequestContext,
    args: Vec<String>,
) -> bool {
    let (engine, ast) = match rhai.get_exec_global(script_id) {
        Ok(lfn) => lfn,
        Err(e) => {
            log::error!(
                "Can't load Rhai top level scrip by id:{script_id} path:{} {e:?}",
                rref.as_string()
            );
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
            log::error!("Can't load Rhai matcher by path:{} {e:?}", rref.as_string());
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
