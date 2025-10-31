use std::{env, io::Read as _};

use serde::{Deserialize, Serialize};

use crate::{DEFAULT_PORT, deceit::Deceit};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApateSpecs {
    pub deceit: Vec<Deceit>,
}

#[derive(Debug)]
pub struct AppConfig {
    pub port: u16,
    pub specs: ApateSpecs,
}

impl AppConfig {
    pub fn try_new_defaults() -> anyhow::Result<Self> {
        Self::try_new(Some(DEFAULT_PORT), Vec::new())
    }

    pub fn try_new(port: Option<u16>, specs_files: Vec<String>) -> anyhow::Result<Self> {
        let port = if let Some(p) = port {
            p
        } else {
            env::var("APATHE_PORT")
                .map(|p| p.parse::<u16>().unwrap())
                .unwrap_or(DEFAULT_PORT)
        };

        let specs = Self::read_specs(specs_files)?;

        Ok(AppConfig {
            port,
            specs,
            // rust_log: env::var("RUST_LOG").unwrap_or("info,api_stub_server=debug".into()),
        })
    }

    fn read_specs(specs_files: Vec<String>) -> anyhow::Result<ApateSpecs> {
        let mut specs = ApateSpecs::default();

        for path in specs_files {
            let stub = Self::parse_specs_from(&path)?;
            specs.deceit.extend(stub.deceit);
        }

        for path in Self::read_paths_from_env() {
            let stub = Self::parse_specs_from(&path)?;
            specs.deceit.extend(stub.deceit);
        }
        Ok(specs)
    }

    fn parse_specs_from(path: &str) -> anyhow::Result<ApateSpecs> {
        log::debug!("Parsing TOML config from: {}", path);

        let mut file =
            std::fs::File::open(path).map_err(|e| anyhow::anyhow!("Can't parse {path}. {e}"))?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let specs: ApateSpecs = toml::from_slice(&buf)?;
        Ok(specs)
    }

    fn read_paths_from_env() -> Vec<String> {
        env::vars()
            .filter_map(|(key, value)| {
                if key.starts_with("APATHE_SPECS_FILE") {
                    Some(value)
                } else {
                    None
                }
            })
            .collect()
    }
}

pub struct AppConfigBuilder {
    port: u16,
    deceit: Vec<Deceit>,
}

impl Default for AppConfigBuilder {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            deceit: Default::default(),
        }
    }
}

impl AppConfigBuilder {
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn add_deceit(mut self, deceit: Deceit) -> Self {
        self.deceit.push(deceit);
        self
    }

    pub fn build(self) -> AppConfig {
        AppConfig {
            port: self.port,
            specs: ApateSpecs {
                deceit: self.deceit,
            },
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    const TOML_TEST: &str = include_str!("../examples/apate-specs.toml");

    /// Just checking that examples toml is valid
    #[test]
    fn deserialize_examples_toml() {
        let specs: ApateSpecs = toml::from_str(TOML_TEST).unwrap();

        // The coolest debug approach ever
        println!("{specs:#?}");
    }
}
