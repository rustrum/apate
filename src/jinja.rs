use std::{collections::HashMap, sync::Arc};

use minijinja::Environment;
use rand::{Rng as _, RngCore as _};
use serde::Serialize;
use uuid::Uuid;

use crate::deceit::DeceitResponseContext;

#[derive(Serialize)]
pub struct MiniJinjaResponseContext<'a> {
    pub path: &'a str,

    pub headers: &'a HashMap<String, String>,

    pub query_args: &'a HashMap<String, String>,

    pub path_args: &'a HashMap<String, String>,

    pub request_json: &'a Option<serde_json::Value>,
}

impl<'a> MiniJinjaResponseContext<'a> {
    pub fn new(ctx: &'a DeceitResponseContext) -> Self {
        Self {
            path: ctx.path.as_str(),
            headers: &ctx.headers,
            query_args: &ctx.query_args,
            path_args: &ctx.path_args,
            request_json: &ctx.request_json,
        }
    }
}

// impl<'a> Object for MiniJinjaResponseContext<'a> {

// }

/// Holds cached minijinja environment.
///
/// Performance improvements are very small here.
/// In my tests is like 4020 req/sec vs 4270 req/sec.
/// Maybe with more complex templates difference will be more noticeable.
///
/// TODO: Maybe remove it to do not add additional complexity?
#[derive(Default, Clone)]
pub struct MiniJinjaState {
    env: Arc<std::sync::RwLock<Option<Environment<'static>>>>,
}

impl MiniJinjaState {
    pub fn get_minijinja(&self) -> Environment<'static> {
        self.init_minijinja_if_not();
        let read_guard = self.env.read().expect("RwLock failed");
        read_guard.clone().expect("Minijinja must exists here")
    }

    pub fn add_minijinja_template(&self, name: &str, source: &str) -> Result<(), minijinja::Error> {
        self.init_minijinja_if_not();
        let mut write_guard = self.env.write().expect("RwLock failed");
        let env = write_guard
            .as_mut()
            .expect("Minijinja env must exists here");
        let tpl = env.get_template(name);
        if tpl.is_err() {
            env.add_template_owned(name.to_string(), source.to_string())
        } else {
            Ok(())
        }
    }

    fn init_minijinja_if_not(&self) {
        let read_guard = self.env.read().expect("RwLock failed");
        if read_guard.is_none() {
            drop(read_guard);
            let mut write_guard = self.env.write().expect("RwLock failed");
            if write_guard.is_none() {
                *write_guard = Some(init_minijinja());
            }
        }
    }

    pub fn clear(&self) {
        let mut write_guard = self.env.write().expect("Write RwLock failed");
        *write_guard = None;
    }
}

pub(crate) fn init_minijinja() -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    // env.set_trim_blocks(true);
    // env.set_lstrip_blocks(true);
    // env.set_keep_trailing_newline(false);
    add_clean_functions(&mut env);
    env
}

/// Add clean functions (without side effects) to minijinja environment.
pub fn add_clean_functions(env: &mut minijinja::Environment) {
    env.add_function("random_num", ctx_random_num);
    env.add_function("random_hex", ctx_random_hex);
    env.add_function("uuid_v4", ctx_uuid_v4);
}

fn ctx_random_num(a: Option<u128>, b: Option<u128>) -> String {
    let Some(first) = a else {
        return rand::random::<u128>().to_string();
    };

    let Some(second) = b else {
        let num = rand::random::<u128>() % first;
        return num.to_string();
    };

    rand::rng().random_range(first..second).to_string()
}

fn ctx_random_hex(length: Option<u64>) -> String {
    let bytes_num = length.unwrap_or(32) as usize;
    let mut bytes = vec![0u8; bytes_num];
    rand::rng().fill_bytes(&mut bytes);

    hex::encode(bytes)
}

fn ctx_uuid_v4() -> String {
    Uuid::new_v4().to_string()
}
