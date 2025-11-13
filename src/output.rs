//! This module responsibility is to build HTTP response message body

use std::sync::{Arc, atomic::Ordering};

use base64::Engine as _;
use minijinja::Environment;
use rand::{Rng as _, RngCore as _};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::deceit::DeceitResponseContext;

/// Define an approach how to handle `output` property from configuration.
/// Result will be placed in HTTP response message body.
#[derive(Default, Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    /// Handle output as minijinja template
    #[default]
    Jinja,
    /// Handle output as binary data that will be decoded from HEX string (no 0x prefix expected).
    Hex,
    // #[serde(rename = "base64")]
    /// Handle output as binary data that will be decoded from Base64 string.
    Base64,
}

/// Holds cached minijinja environment.
///
/// Performance improvements are very small here.
/// In my tests is like 4020 req/sec vs 4270 req/sec.
/// Maybe with more complex templates difference will be more noticeable.
///
/// TODO: Remove it to do not add additional complexity?
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

pub fn build_response_body(
    tp: OutputType,
    output: &str,
    ctx: &DeceitResponseContext,
) -> color_eyre::Result<Vec<u8>> {
    match tp {
        OutputType::Jinja => prepare_jinja_output(output, ctx),
        OutputType::Hex => {
            let hex_str = output.trim().strip_prefix("0x").unwrap_or(output).trim();
            Ok(hex::decode(hex_str)?)
        }
        OutputType::Base64 => Ok(base64::prelude::BASE64_STANDARD.decode(output.trim())?),
    }
}

pub fn prepare_jinja_output(
    template: &str,
    ctx: &DeceitResponseContext,
) -> color_eyre::Result<Vec<u8>> {
    // Old way no cache
    // let mut env = init_minijinja();
    // let tpl_id = template_id(template);
    // env.add_template(&tpl_id, template)?;

    let tpl_id = template_id(template);
    ctx.minijinja.add_minijinja_template(&tpl_id, template)?;
    let mut env = ctx.minijinja.get_minijinja();

    let counters = ctx.counters.clone();
    env.add_function(
        "get_counter",
        move |key: &str| -> Result<u64, minijinja::Error> {
            let value = counters.get_and_increment(key).map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::UndefinedError,
                    format!("Can't get counter value for key \"{key}\". {e:?}"),
                )
            })?;
            Ok(value)
        },
    );

    let force_response_code = ctx.response_code.clone();
    env.add_function("force_response_code", move |code: u16| {
        force_response_code.store(code, Ordering::Relaxed);
    });

    let tpl = env.get_template(&tpl_id)?;
    let response = tpl
        .render(ctx)
        .map_err(|e| color_eyre::eyre::eyre!("Can't render minijinja template: {e}"))?;

    Ok(response.into_bytes())
}

pub(crate) fn init_minijinja() -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    // env.set_trim_blocks(true);
    // env.set_lstrip_blocks(true);
    // env.set_keep_trailing_newline(false);
    add_clean_functions(&mut env);
    env
}

fn template_id(content: &str) -> String {
    let hash1: u64 = cityhasher::hash_with_seed(content, 42);
    let hash2: u64 = cityhasher::hash_with_seed(content, 69);
    let mut u128bytes = [0u8; 16];
    u128bytes[0..8].copy_from_slice(&hash1.to_be_bytes());
    u128bytes[8..16].copy_from_slice(&hash2.to_be_bytes());
    u128::from_be_bytes(u128bytes).to_string()
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
