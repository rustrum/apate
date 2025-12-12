use std::{
    collections::HashMap,
    sync::{Arc, RwLock, atomic::Ordering},
};

use rhai::{
    AST, Blob, Dynamic, Engine, EvalAltResult, Map as RhaiMap, ParseError, ParseErrorType, Position,
};
use serde::{Deserialize, Serialize};

use crate::{RequestContext, deceit::DeceitResponseContext};

/// Thai script specification that can be used as a matcher or processor.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RhaiScript {
    pub id: String,
    pub script: String,
}

#[derive(Clone)]
pub struct RhaiState {
    engine: Arc<Engine>,
    scripts: Arc<RwLock<HashMap<String, String>>>,
    asts: Arc<RwLock<HashMap<String, Arc<AST>>>>,
}
impl Default for RhaiState {
    fn default() -> Self {
        Self {
            engine: Arc::new(build_rhai_engine()),
            scripts: Default::default(),
            asts: Default::default(),
        }
    }
}

impl RhaiState {
    pub fn get_exec_global(&self, script_id: &str) -> Result<(Arc<Engine>, Arc<AST>), ParseError> {
        let bytecode_id = format!("global:{script_id}");
        let rguard = self.asts.read().expect("Rhai RwLock read failed");
        let Some(entry) = rguard.get(&bytecode_id) else {
            drop(rguard);
            let sguard = self.scripts.read().expect("Rhai RwLock read failed");
            let Some(script) = sguard.get(script_id) else {
                return Err(ParseError(
                    ParseErrorType::ModuleUndefined(format!("No Rhai script with id: {script_id}"))
                        .into(),
                    Position::NONE,
                ));
            };
            let ast = self.rhai_build_ast(&self.engine, bytecode_id, script)?;
            return Ok((self.engine.clone(), ast));
        };
        Ok((self.engine.clone(), entry.clone()))
    }

    pub fn get_exec(
        &self,
        id: String,
        script: &str,
    ) -> Result<(Arc<Engine>, Arc<AST>), ParseError> {
        let rguard = self.asts.read().expect("Rhai RwLock read failed");
        let Some(entry) = rguard.get(&id) else {
            drop(rguard);
            let ast = self.rhai_build_ast(&self.engine, id, script)?;
            return Ok((self.engine.clone(), ast));
        };
        Ok((self.engine.clone(), entry.clone()))
    }

    fn rhai_build_ast(
        &self,
        rhai: &Engine,
        id: String,
        script: &str,
    ) -> Result<Arc<AST>, ParseError> {
        let script_ast = rhai.compile(script)?;
        let mut wguard = self.asts.write().expect("Write guard for Rhai failed");
        let ast = Arc::new(script_ast);
        (*wguard).insert(id, ast.clone());
        Ok(ast)
    }

    fn clear(&self) {
        let mut bytecode_guard = self.asts.write().expect("Write guard for Rhai failed");
        (*bytecode_guard).clear();

        let mut scripts_guard = self.scripts.write().expect("Write guard for Rhai failed");
        (*scripts_guard).clear();
    }

    pub fn clear_and_update(&self, scripts: Vec<RhaiScript>) {
        self.clear();

        let mapped_scripts = scripts.into_iter().map(|v| (v.id, v.script)).collect();
        let mut scripts_guard = self.scripts.write().expect("Write guard for Rhai failed");
        // log::debug!("Rhai scripts:\n{mapped_scripts:?}");
        *scripts_guard = mapped_scripts;
    }
}

/// Context available in Rhai matchers under `ctx` variable.
///
/// Expose next API:
///  - ctx.method -> returns request method
///  - ctx.path -> returns request path
///  - ctx.load_headers() -> build request headers map (lowercase keys)
///  - ctx.load_query_args() -> build map with URL query arguments
///  - ctx.load_path_args() -> build arguments map from specs URIs like /mypath/{user_id}/{item_id}
///  - ctx.load_body() -> reads request body as Blob
#[derive(Debug, Clone)]
pub struct RhaiRequestContext {
    pub req: RequestContext,
}

impl RhaiRequestContext {
    pub fn get_method(&mut self) -> String {
        self.req.method.clone()
    }

    pub fn get_path(&mut self) -> String {
        self.req.path.as_ref().clone()
    }

    pub fn load_headers(&mut self) -> RhaiMap {
        self.req
            .headers
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_path_args(&mut self) -> RhaiMap {
        self.req
            .path_args
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_query_args(&mut self) -> RhaiMap {
        self.req
            .query_args
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_body(&mut self) -> Blob {
        Blob::from(self.req.body.to_vec())
    }
}

impl From<RequestContext> for RhaiRequestContext {
    fn from(ctx: RequestContext) -> Self {
        Self { req: ctx.clone() }
    }
}

/// Context available in Rhai processors under `ctx` variable.
///
/// Expose next API:
///  - ctx.method -> returns request method
///  - ctx.path -> returns request path
///  - ctx.response_code -> get set custom response code if any (default 0 if not set)
///  - ctx.load_headers() -> build request headers map (lowercase keys)
///  - ctx.load_query_args() -> build map with URL query arguments
///  - ctx.load_path_args() -> build arguments map from specs URIs like /mypath/{user_id}/{item_id}
///  - ctx.load_body() -> reads request body as Blob
///  - ctx.inc_counter("key") -> increment counter by key and returns previous value
#[derive(Clone)]
pub struct RhaiResponseContext {
    ctx: DeceitResponseContext,
}

impl From<DeceitResponseContext> for RhaiResponseContext {
    fn from(ctx: DeceitResponseContext) -> Self {
        Self { ctx }
    }
}

impl RhaiResponseContext {
    pub fn get_method(&mut self) -> String {
        self.ctx.req.method.to_string()
    }

    pub fn get_path(&mut self) -> String {
        self.ctx.req.path.to_string()
    }

    pub fn get_response_code(&mut self) -> i64 {
        self.ctx.response_code.load(Ordering::Relaxed) as i64
    }

    pub fn set_response_code(&mut self, value: i64) {
        self.ctx
            .response_code
            .store(value as u16, Ordering::Relaxed);
    }

    pub fn inc_counter(&mut self, key: &str) -> Result<u64, Box<EvalAltResult>> {
        self.ctx.counters.get_and_increment(key).map_err(|e| {
            Box::new(EvalAltResult::ErrorSystem(
                "Failed inc_counter".to_string(),
                e.into(),
            ))
        })
    }

    pub fn load_headers(&mut self) -> RhaiMap {
        self.ctx
            .req
            .headers
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_path_args(&mut self) -> RhaiMap {
        self.ctx
            .req
            .path_args
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_query_args(&mut self) -> RhaiMap {
        self.ctx
            .req
            .query_args
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_body(&mut self) -> Blob {
        Blob::from(self.ctx.req.body.to_vec())
    }
}

fn build_rhai_engine() -> Engine {
    let mut engine = Engine::new();

    engine.register_fn("to_json_blob", to_json_blob);
    engine.register_fn("from_json_blob", from_json_blob);

    engine.on_print(|s| {
        log::info!("RHAI: {s}");
    });

    engine.on_debug(|s, src, pos| {
        log::debug!("RHAI: {} @ {pos:?} > {s}", src.unwrap_or_default());
    });

    engine
        .register_type::<RhaiRequestContext>()
        .register_get("method", RhaiRequestContext::get_method)
        .register_get("path", RhaiRequestContext::get_path)
        .register_fn("load_headers", RhaiRequestContext::load_headers)
        .register_fn("load_query_args", RhaiRequestContext::load_query_args)
        .register_fn("load_path_args", RhaiRequestContext::load_path_args)
        .register_fn("load_body", RhaiRequestContext::load_body);

    engine
        .register_type::<RhaiResponseContext>()
        .register_get("method", RhaiResponseContext::get_method)
        .register_get("path", RhaiResponseContext::get_path)
        .register_fn("inc_counter", RhaiResponseContext::inc_counter)
        .register_get_set(
            "response_code",
            RhaiResponseContext::get_response_code,
            RhaiResponseContext::set_response_code,
        )
        .register_fn("load_headers", RhaiResponseContext::load_headers)
        .register_fn("load_query_args", RhaiResponseContext::load_query_args)
        .register_fn("load_path_args", RhaiResponseContext::load_path_args)
        .register_fn("load_body", RhaiResponseContext::load_body);

    engine
}

fn to_json_blob(value: &mut Dynamic) -> Result<Blob, Box<EvalAltResult>> {
    serde_json::to_string(value)
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorSystem(
                "Can't convert to JSON string".to_string(),
                Box::new(e),
            ))
        })
        .map(Blob::from)
}

fn from_json_blob(value: &mut Blob) -> Result<Dynamic, Box<EvalAltResult>> {
    serde_json::from_slice::<Dynamic>(value).map_err(|e| {
        Box::new(EvalAltResult::ErrorSystem(
            "Can't decode JSON from bytes".to_string(),
            Box::new(e),
        ))
    })
}
