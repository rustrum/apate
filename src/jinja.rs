use std::{
    fmt::Debug,
    sync::{Arc, atomic::Ordering},
};

use minijinja::{
    Environment, State, Value, context,
    value::{Object, ObjectRepr},
};
use rand::{Rng as _, RngCore as _};
use uuid::Uuid;

use crate::deceit::DeceitResponseContext;

/// Response context for mininijinja templates that is available under `ctx` variable.
///
/// Expose next API:
///  - ctx.method -> returns request method
///  - ctx.path -> returns request path
///  - ctx.response_code -> get set custom response code if any (default 0 if not set)
///  - ctx.load_headers() -> build request headers map (lowercase keys)
///  - ctx.load_query_args() -> build map with URL query arguments
///  - ctx.load_path_args() -> build arguments map from specs URIs like /mypath/{user_id}/{item_id}
///  - ctx.load_body_string() -> load request body as string
///  - ctx.load_body_json() -> load request body as json
///  - ctx.inc_counter("key") -> increment counter by key and returns previous value
pub struct MiniJinjaResponseContext {
    ctx: DeceitResponseContext,
}

impl Debug for MiniJinjaResponseContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniJinjaResponseContext")
            .field("ctx", &"!debug_not_supported!")
            .finish()
    }
}

impl MiniJinjaResponseContext {
    pub fn new(ctx: DeceitResponseContext) -> Self {
        Self { ctx }
    }
}
impl Object for MiniJinjaResponseContext {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn get_value(self: &Arc<Self>, field: &Value) -> Option<Value> {
        match field.as_str()? {
            "method" => Some(Value::from(self.ctx.method.as_str())),
            "path" => Some(Value::from(self.ctx.path.as_str())),
            _ => None,
        }
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &State<'_, '_>,
        method: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match method {
            "load_headers" => Ok(Value::from(self.ctx.headers.as_ref().clone())),
            "load_query_args" => Ok(Value::from(self.ctx.query_args.as_ref().clone())),
            "load_path_args" => Ok(Value::from(self.ctx.path_args.as_ref().clone())),
            "load_body_string" => {
                if self.ctx.request_body.trim_ascii().is_empty() {
                    Ok(Value::default())
                } else {
                    let body = String::from_utf8_lossy(self.ctx.request_body.as_ref());
                    Ok(Value::from(body))
                }
            }
            "load_body_json" => match self.ctx.load_request_json() {
                Ok(v) => Ok(Value::from_serialize(v.as_ref())),
                Err(e) => {
                    log::error!("Can't parse response body as JSON: {e}");
                    Err(minijinja::Error::from(minijinja::ErrorKind::CannotUnpack))
                }
            },
            "inc_counter" => {
                if args.len() != 1 {
                    return Err(minijinja::Error::from(
                        minijinja::ErrorKind::MissingArgument,
                    ));
                }
                let Some(key) = args[0].as_str() else {
                    return Err(minijinja::Error::from(minijinja::ErrorKind::NonKey));
                };
                self.ctx
                    .counters
                    .get_and_increment(key)
                    .map(Value::from)
                    .map_err(|e| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::UndefinedError,
                            format!("Can't get counter value for key \"{key}\". {e:?}"),
                        )
                    })
            }
            "set_response_code" => {
                if args.len() != 1 {
                    return Err(minijinja::Error::from(
                        minijinja::ErrorKind::MissingArgument,
                    ));
                }
                let Some(code) = args[0].as_i64() else {
                    return Err(minijinja::Error::from(minijinja::ErrorKind::NonPrimitive));
                };
                self.ctx.response_code.store(code as u16, Ordering::Relaxed);
                Ok(Value::default())
            }
            _ => Err(minijinja::Error::from(minijinja::ErrorKind::UnknownMethod)),
        }
    }
}

pub fn build_tpl_context(ctx: DeceitResponseContext) -> minijinja::Value {
    let mjctx = MiniJinjaResponseContext::new(ctx);
    context! {
       ctx => Value::from_object(mjctx)
    }
}

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
