use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use actix_web::web::Bytes;
use rhai::{AST, Engine, Map as RhaiMap, ParseError, ParseErrorType, Position};
use serde::{Deserialize, Serialize};

use crate::RequestContext;

/// Thai script specification that can be used as a matcher or processor.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RhaiScript {
    pub id: String,
    pub script: String,
}

/// TODO implement properly
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

#[derive(Debug, Clone)]
pub struct RhaiRequestContext {
    pub method: String,
    pub headers: RhaiMap,
    pub body: Arc<Bytes>,
    pub path: Arc<String>,
    pub args_query: Arc<HashMap<String, String>>,
    pub args_path: Arc<HashMap<String, String>>,
}

impl RhaiRequestContext {
    pub fn get_method(&mut self) -> String {
        self.method.clone()
    }

    pub fn get_path(&mut self) -> String {
        self.path.as_ref().clone()
    }

    pub fn load_headers(&mut self) -> RhaiMap {
        self.headers.clone()
    }

    pub fn load_path_args(&mut self) -> RhaiMap {
        self.args_path
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn load_query_args(&mut self) -> RhaiMap {
        self.args_query
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }
}

impl From<RequestContext> for RhaiRequestContext {
    fn from(ctx: RequestContext) -> Self {
        let headers = ctx
            .headers
            .iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        let path = Arc::new(ctx.path.as_str().to_string());
        Self {
            method: ctx.method.clone(),
            headers,
            body: ctx.body.clone(),
            path,
            args_query: ctx.args_query.clone(),
            args_path: ctx.args_path.clone(),
        }
    }
}

fn build_rhai_engine() -> Engine {
    let mut engine = Engine::new();

    engine.on_print(move |s| {
        log::info!("RHAI: {s}");
    });

    engine.on_debug(move |s, src, pos| {
        log::debug!("RHAI: {} @ {pos:?} > {s}", src.unwrap_or_default());
    });

    engine
        .register_type::<RhaiRequestContext>()
        .register_get("method", RhaiRequestContext::get_method)
        .register_get("path", RhaiRequestContext::get_path)
        .register_fn("load_headers", RhaiRequestContext::load_headers)
        .register_fn("load_query_args", RhaiRequestContext::load_query_args)
        .register_fn("load_path_args", RhaiRequestContext::load_path_args);

    engine
}
