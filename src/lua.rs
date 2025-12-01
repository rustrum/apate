use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use mlua::{Function, Lua, UserData, UserDataMethods};
use serde::{Deserialize, Serialize};

use crate::{RequestContext, deceit::DeceitResponseContext};

thread_local! {
    static LUA: Lua = build_lua_env();
}

/// It is used in thread_local but linter does not see it
#[allow(dead_code)]
fn build_lua_env() -> Lua {
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
            "log_warn",
            lua.create_function(|_, (msg,): (String,)| {
                log::warn!("LUA: {msg}");
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();
    lua
}

/// LUA script specification that can be used as a matcher or processor.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LuaScript {
    pub id: String,
    pub script: String,
}

/// TODO implement properly
#[derive(Clone, Default)]
pub struct LuaState {
    scripts: Arc<RwLock<HashMap<String, String>>>,
    bytecode: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl LuaState {
    /// Returns top level lua script from specification by it's id.
    pub fn get_lua_script(&self, script_id: &str) -> mlua::Result<Function> {
        let bytecode_id = format!("global:{script_id}");
        LUA.with(|lua| {
            let rguard = self.bytecode.read().expect("LUA RwLock read failed");
            let Some(entry) = rguard.get(&bytecode_id) else {
                drop(rguard);
                let sguard = self.scripts.read().expect("LUA RwLock read failed");
                let Some(script) = sguard.get(script_id) else {
                    return Err(mlua::Error::runtime(format!(
                        "Can't load top level LUA script by id: {script_id}"
                    )));
                };
                let lua_fn = self.init_lua_bytecode(lua, bytecode_id, script)?;
                return Ok(lua_fn);
            };
            lua.load(entry).into_function()
        })
    }

    pub fn to_lua_function(&self, id: String, script: &str) -> mlua::Result<Function> {
        LUA.with(|lua| {
            let rguard = self.bytecode.read().expect("LUA RwLock read failed");
            let Some(entry) = rguard.get(&id) else {
                drop(rguard);
                let lua_fn = self.init_lua_bytecode(lua, id, script)?;
                return Ok(lua_fn);
            };
            lua.load(entry).into_function()
        })
    }

    fn init_lua_bytecode(&self, lua: &Lua, id: String, script: &str) -> mlua::Result<Function> {
        let lua_fn = lua.load(script).into_function()?;
        let mut wguard = self.bytecode.write().expect("Write guard for LUAs failed");
        (*wguard).insert(id, lua_fn.dump(false));
        Ok(lua_fn)
    }

    pub fn clear_and_update(&self, scripts: Vec<LuaScript>) {
        self.clear();

        let mapped_scripts = scripts.into_iter().map(|v| (v.id, v.script)).collect();
        let mut scripts_guard = self.scripts.write().expect("Write guard for LUAs failed");
        log::info!("SCRIPTS: {mapped_scripts:?}");
        *scripts_guard = mapped_scripts;
    }

    fn clear(&self) {
        let mut bytecode_guard = self.bytecode.write().expect("Write guard for LUAs failed");
        (*bytecode_guard).clear();

        let mut scripts_guard = self.scripts.write().expect("Write guard for LUAs failed");
        (*scripts_guard).clear();
    }
}

/// Passed as a first argument into lua matchers.
///
/// Provides access to [`RequestContext`] via next methods:
///  - ctx:path()
///  - ctx:method()
///  - ctx:get_header(header_name)
///  - ctx:get_query_arg(query_arg_name)
///  - ctx:get_path_arg(query_arg_name)
#[derive(Debug)]
pub struct LuaRequestContext {
    ctx: RequestContext,
}

impl From<RequestContext> for LuaRequestContext {
    fn from(ctx: RequestContext) -> Self {
        Self { ctx }
    }
}

/// [`UserData`] does not work with mlua "send" feature because [`RequestContext`] is not send.
impl UserData for LuaRequestContext {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("path", |_, this, ()| {
            let path = this.ctx.path.as_str().to_string();
            Ok(path)
        });

        methods.add_method("method", |_, this, ()| {
            let path = this.ctx.req.method().to_string();
            Ok(path)
        });

        methods.add_method("get_query_arg", |_, this, key: String| {
            Ok(this.ctx.args_query.get(&key).cloned())
        });

        methods.add_method("get_path_arg", |_, this, key: String| {
            Ok(this.ctx.args_path.get(&key).cloned())
        });

        methods.add_method("get_header", |_, this, key: String| {
            let result = if let Some(header) = this.ctx.req.headers().get(&key) {
                let mapped = header.to_str().map_err(mlua::Error::external)?.to_string();
                Some(mapped)
            } else {
                None
            };
            Ok(result)
        });
    }
}

#[allow(unused)]
pub struct LuaResponseContext {
    ctx: DeceitResponseContext,
}

impl From<DeceitResponseContext> for LuaResponseContext {
    fn from(ctx: DeceitResponseContext) -> Self {
        Self { ctx }
    }
}

impl UserData for LuaResponseContext {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {
        // TODO: add methods
    }
}
