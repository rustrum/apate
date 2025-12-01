use std::{collections::HashMap, fmt::Debug};

use color_eyre::eyre::eyre;
use mlua::Function;
use serde::{Deserialize, Serialize};

use crate::{
    ResourceRef,
    deceit::DeceitResponseContext,
    lua::{LuaResponseContext, LuaState},
};

/// Trait for custom user-defined logic to run after output response is prepared (rendered).
pub trait PostProcessor: Sync + Send {
    fn process(
        &self,
        input: &str,
        context: &DeceitResponseContext,
        response: &[u8],
    ) -> Result<Option<Vec<u8>>, Box<dyn core::error::Error>>;
}

/// Custom logic to execute after output content was prepared (rendered).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Processor {
    Lua {
        script: String,
    },
    LuaScript {
        id: String,
        args: Vec<String>,
    },
    /// References to custom user processor.
    Custom {
        /// Processor with this ID should be added on server initialization.
        id: String,

        /// Custom user input understandable only by processor logic.
        #[serde(default)]
        input: String,
    },
}

// #[derive(Clone, Debug, Deserialize, Serialize)]
// pub enum ProcessorScope {
//     Pre,
//     Post,
// }

pub struct ApateProcessor {
    pub id: String,
    pub post: Box<dyn PostProcessor>,
}

impl ApateProcessor {
    /// Creates post processor.
    pub fn post(id: &str, callback: Box<dyn PostProcessor>) -> Self {
        Self {
            id: id.to_string(),
            post: callback,
        }
    }

    pub fn apply_post(
        &self,
        input: &str,
        rctx: &DeceitResponseContext,
        body: &[u8],
    ) -> color_eyre::Result<Option<Vec<u8>>> {
        (*self.post)
            .process(input, rctx, body)
            .map_err(|e| color_eyre::eyre::eyre!("Post processor execution failed: {e}"))
    }
}

impl Debug for ApateProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApateProcessor")
            .field("id", &self.id)
            .finish()
    }
}

pub(crate) fn apply_processors(
    rref: &ResourceRef,
    custom_registry: &HashMap<String, ApateProcessor>,
    processors: &[&Processor],
    rctx: &DeceitResponseContext,
    body: &[u8],
    lua: &LuaState,
) -> color_eyre::Result<Option<Vec<u8>>> {
    let mut result: Option<Vec<u8>> = None;

    for (pid, p) in processors.iter().enumerate() {
        let input_bytes = if let Some(bts) = result.as_ref() {
            bts
        } else {
            body
        };

        let processor_ref = rref.with_level(pid);

        match p {
            Processor::Custom { id, input } => {
                let Some(p) = custom_registry.get(id.as_str()) else {
                    color_eyre::eyre::bail!("Can't get processor by id \"{id}\"");
                };
                if let Some(new_body) = p.apply_post(input.as_str(), rctx, input_bytes)? {
                    result = Some(new_body);
                }
            }
            Processor::Lua { script } => {
                if let Some(new_body) =
                    apply_lua_processor(lua, processor_ref, script.as_str(), rctx.clone(), body)?
                {
                    result = Some(new_body)
                }
            }
            Processor::LuaScript { id, args } => {
                if let Some(new_body) = apply_lua_processor_script(
                    lua,
                    processor_ref,
                    id.as_str(),
                    args.clone(),
                    rctx.clone(),
                    body,
                )? {
                    result = Some(new_body)
                }
            }
        }
    }

    Ok(result)
}

pub(crate) fn apply_lua_processor(
    lua: &LuaState,
    pref: ResourceRef,
    script: &str,
    rctx: DeceitResponseContext,
    body: &[u8],
) -> color_eyre::Result<Option<Vec<u8>>> {
    let id = pref.to_resource_id("lua-processor");

    let lua_fn = lua
        .to_lua_function(id, script)
        .map_err(|e| eyre!("{e:?}"))?;

    apply_lua(lua_fn, rctx.into(), body, vec![])
}

pub(crate) fn apply_lua_processor_script(
    lua: &LuaState,
    _pref: ResourceRef,
    script_id: &str,
    args: Vec<String>,
    rctx: DeceitResponseContext,
    body: &[u8],
) -> color_eyre::Result<Option<Vec<u8>>> {
    let lua_fn = lua.get_lua_script(script_id).map_err(|e| eyre!("{e:?}"))?;

    apply_lua(lua_fn, rctx.into(), body, args)
}

fn apply_lua(
    lua_fn: Function,
    ctx: LuaResponseContext,
    body: &[u8],
    args: Vec<String>,
) -> color_eyre::Result<Option<Vec<u8>>> {
    let value = lua_fn
        .call::<mlua::Value>((ctx, args, body))
        .map_err(|e| eyre!("{e:?}"))?;

    if value.is_nil() || value.is_null() {
        return Ok(None);
    }

    if let Some(r) = value.as_string() {
        Ok(Some(r.as_bytes().to_vec()))
    } else {
        Err(eyre!("Lua processor returned not a string value."))
    }
}
