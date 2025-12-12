//! This module responsibility is to build HTTP response message body
use std::sync::atomic::Ordering;

use base64::Engine as _;
use color_eyre::eyre::{bail, eyre};
use rhai::{AST, Blob, Dynamic, Engine, Scope};
use serde::{Deserialize, Serialize};

use crate::{
    ResourceRef,
    deceit::DeceitResponseContext,
    jinja::{MiniJinjaState, build_tpl_context},
    rhai::{RhaiResponseContext, RhaiState},
};

/// Define an approach how to handle `output` property from configuration.
/// Result will be placed in HTTP response message body.
#[derive(Default, Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    /// Return output string as is.
    #[default]
    String,
    /// Handle output as minijinja template.
    Jinja,
    /// Handle output as binary data that will be decoded from HEX string (no 0x prefix expected).
    Hex,
    // #[serde(rename = "base64")]
    /// Handle output as binary data that will be decoded from Base64 string.
    Base64,
    /// Output is a Rhai script
    Rhai,
}

pub fn output_response_body(
    deceit_ref: &ResourceRef,
    tp: OutputType,
    output: &str,
    ctx: &DeceitResponseContext,
    mini_jinja_state: &MiniJinjaState,
    rhai_state: &RhaiState,
) -> color_eyre::Result<Vec<u8>> {
    match tp {
        OutputType::String => Ok(output.as_bytes().to_vec()),
        OutputType::Jinja => render_using_minijinja(deceit_ref, output, ctx, mini_jinja_state),
        OutputType::Hex => {
            let hex_str = output.trim().strip_prefix("0x").unwrap_or(output).trim();
            Ok(hex::decode(hex_str)?)
        }
        OutputType::Base64 => Ok(base64::prelude::BASE64_STANDARD.decode(output.trim())?),
        OutputType::Rhai => render_using_rhai(deceit_ref, output, ctx, rhai_state),
    }
}

fn render_using_minijinja(
    deceit_ref: &ResourceRef,
    template: &str,
    ctx: &DeceitResponseContext,
    mini_jinja_state: &MiniJinjaState,
) -> color_eyre::Result<Vec<u8>> {
    // Old way no cache
    // let mut env = init_minijinja();
    // let tpl_id = template_id(template);
    // env.add_template(&tpl_id, template)?;

    let id = deceit_ref.to_resource_id("jinja-output");
    mini_jinja_state.add_minijinja_template(&id, template)?;
    let mut env = mini_jinja_state.get_minijinja();

    let force_response_code = ctx.response_code.clone();
    env.add_function("force_response_code", move |code: u16| {
        force_response_code.store(code, Ordering::Relaxed);
    });

    let tpl = env.get_template(&id)?;
    let jinja_ctx = build_tpl_context(ctx.clone());
    let response = tpl
        .render(jinja_ctx)
        .map_err(|e| eyre!("Can't render minijinja template: {e}"))?;

    Ok(response.into_bytes())
}

fn render_using_rhai(
    deceit_ref: &ResourceRef,
    script: &str,
    ctx: &DeceitResponseContext,
    rhai: &RhaiState,
) -> color_eyre::Result<Vec<u8>> {
    let id = deceit_ref.to_resource_id("rhai-output");

    let (engine, ast) = rhai
        .get_exec(id.clone(), script)
        .map_err(|e| eyre!("Can't load Rhai template: {e:?}"))?;

    call_rhai(&engine, &ast, ctx.clone().into())
}

fn call_rhai(engine: &Engine, ast: &AST, ctx: RhaiResponseContext) -> color_eyre::Result<Vec<u8>> {
    let mut scope = Scope::new();
    scope.set_value("ctx", ctx);

    let result = engine.eval_ast_with_scope::<Dynamic>(&mut scope, ast)?;

    let value = if result.is_unit() {
        Default::default()
    } else if result.is_blob() {
        result
            .try_cast_result::<Blob>()
            .map_err(|e| eyre!("Must not happen here {e:?}"))?
    } else {
        bail!("Wrong Rhai template return type: {}", result.type_name());
    };

    Ok(value)
}
