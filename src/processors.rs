use std::{collections::HashMap, fmt::Debug};

use serde::{Deserialize, Serialize};

use crate::deceit::DeceitResponseContext;

// Pre processing looks cool but absolutely useless for now
// type PreProcessor = dyn for<'a> Fn(&DeceitResponseContext<'a>) -> color_eyre::Result<()>;

/// Post processor callback.
/// Accepts:
///
///  - user input as str
///  - current context
///  - response message as bytes
///
/// As a result it could return new response message bytes.
type PostProcessor = dyn for<'a> Fn(&str, &DeceitResponseContext<'a>, &[u8]) -> color_eyre::Result<Option<Vec<u8>>>
    + Sync
    + Send;

/// Set up custom logic that could be executed before/after rendering response.
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
    /// Reserved for custom user processors
    Custom {
        id: String,

        #[serde(default)]
        input: String,
        // scope: ProcessorScope,
    },
}

// #[derive(Clone, Debug, Deserialize, Serialize)]
// pub enum ProcessorScope {
//     Pre,
//     Post,
// }

pub struct ApateProcessor {
    pub id: String,
    pub post: Box<PostProcessor>,
}

impl ApateProcessor {
    pub fn post(id: &str, callback: Box<PostProcessor>) -> Self {
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
        (*self.post)(input, rctx, body)
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
