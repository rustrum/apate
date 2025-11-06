use std::{collections::HashMap, fmt::Debug};

use serde::{Deserialize, Serialize};

use crate::deceit::DeceitResponseContext;

/// Trait for custom user-defined logic to run after output response is prepared (rendered).
pub trait PostProcessor: Sync + Send {
    fn process<'a>(
        &self,
        input: &str,
        context: &DeceitResponseContext<'a>,
        response: &[u8],
    ) -> Result<Option<Vec<u8>>, Box<dyn core::error::Error>>;
}

/// Custom logic to execute after output content was prepared (rendered).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Processor {
    // Lua {
    //     scope: ProcessorScope,
    //     script: String,
    // },
    // LuaFile {
    //     scope: ProcessorScope,
    //     path: String,
    // },
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

    pub fn apply_post<'a>(
        &self,
        input: &str,
        rctx: &DeceitResponseContext<'a>,
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

pub(crate) fn apply_processors<'a>(
    custom_registry: &HashMap<String, ApateProcessor>,
    processors: &[&Processor],
    rctx: &DeceitResponseContext<'a>,
    body: &[u8],
) -> color_eyre::Result<Option<Vec<u8>>> {
    let mut result: Option<Vec<u8>> = None;

    for p in processors {
        let input_bytes = if let Some(bts) = result.as_ref() {
            bts
        } else {
            body
        };

        match p {
            Processor::Custom { id, input } => {
                let Some(p) = custom_registry.get(id.as_str()) else {
                    color_eyre::eyre::bail!("Can't get processor by id \"{id}\"");
                };
                if let Some(new_body) = p.apply_post(input.as_str(), rctx, input_bytes)? {
                    result = Some(new_body);
                }
            }
        }
    }

    Ok(result)
}
