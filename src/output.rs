//! This module responsibility is to build HTTP response message body
use std::sync::atomic::Ordering;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::{
    ResourceRef,
    deceit::DeceitResponseContext,
    jinja::{MiniJinjaState, build_tpl_context},
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
}

pub fn output_response_body(
    deceit_ref: &ResourceRef,
    tp: OutputType,
    output: &str,
    ctx: &DeceitResponseContext,
    mini_jinja_state: &MiniJinjaState,
) -> color_eyre::Result<Vec<u8>> {
    match tp {
        OutputType::String => Ok(output.as_bytes().to_vec()),
        OutputType::Jinja => render_using_minijinja(deceit_ref, output, ctx, mini_jinja_state),
        OutputType::Hex => {
            let hex_str = output.trim().strip_prefix("0x").unwrap_or(output).trim();
            Ok(hex::decode(hex_str)?)
        }
        OutputType::Base64 => Ok(base64::prelude::BASE64_STANDARD.decode(output.trim())?),
    }
}

pub fn render_using_minijinja(
    deceit_ref: &ResourceRef,
    template: &str,
    ctx: &DeceitResponseContext,
    mini_jinja_state: &MiniJinjaState,
) -> color_eyre::Result<Vec<u8>> {
    // Old way no cache
    // let mut env = init_minijinja();
    // let tpl_id = template_id(template);
    // env.add_template(&tpl_id, template)?;

    let tpl_id = deceit_ref.to_resource_id("output");
    mini_jinja_state.add_minijinja_template(&tpl_id, template)?;
    let mut env = mini_jinja_state.get_minijinja();

    let force_response_code = ctx.response_code.clone();
    env.add_function("force_response_code", move |code: u16| {
        force_response_code.store(code, Ordering::Relaxed);
    });

    let tpl = env.get_template(&tpl_id)?;
    let jinja_ctx = build_tpl_context(ctx.clone());
    let response = tpl
        .render(jinja_ctx)
        .map_err(|e| color_eyre::eyre::eyre!("Can't render minijinja template: {e}"))?;

    Ok(response.into_bytes())
}
