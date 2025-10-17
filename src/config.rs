use std::{env, io::Read as _};

use serde::{Deserialize, Serialize};

use crate::deceit::Deceit;

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
    pub fn try_new() -> anyhow::Result<Self> {
        let specs = Self::read_specs()?;
        Ok(AppConfig {
            port: env::var("APATHE_PORT")
                .map(|p| p.parse::<u16>().unwrap())
                .unwrap_or(8042),
            specs,
            // rust_log: env::var("RUST_LOG").unwrap_or("info,api_stub_server=debug".into()),
        })
    }

    fn read_specs() -> anyhow::Result<ApateSpecs> {
        let mut specs = ApateSpecs::default();
        for path in Self::read_paths_from_env() {
            log::debug!("Parsing TOML config from: {}", path);

            let mut file = std::fs::File::open(path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;

            let stub: ApateSpecs = toml::from_slice(&buf)?;
            specs.deceit.extend(stub.deceit);
        }
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

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    #[ignore]
    fn serialize_toml() {
        // let mut deceit = UrisDeceit::new();

        // deceit.insert(
        //     "/path".to_string(),
        //     vec![UriDeceit {
        //         uri: "aaa".to_string(),
        //         matchers: vec![
        //             Matcher::Method {
        //                 eq: "POST".to_string(),
        //             },
        //             Matcher::Header {
        //                 key: "Content-Type".to_string(),
        //                 value: "application/json".to_string(),
        //             },
        //         ],
        //         response_headers: Default::default(),
        //         response_code: Some(200),
        //         responses: vec![DeceitResponse {
        //             code: 200,
        //             matchers: vec![],
        //             post_processors: vec![],
        //             response_headers: Default::default(),
        //             json_request_context: false,
        //             response_template: "Template multiline".to_string(),
        //         }],
        //     }],
        // );

        // let mut specs = ApateSpecs {
        //     deceit: vec![
        //         UriDeceit {
        //             uri: "aaa".to_string(),
        //             matchers: vec![
        //                 Matcher::Method {
        //                     eq: "POST".to_string(),
        //                 },
        //                 Matcher::Header {
        //                     key: "Content-Type".to_string(),
        //                     value: "application/json".to_string(),
        //                 },
        //             ],
        //             response_headers: Default::default(),
        //             response_code: Some(200),
        //             responses: vec![DeceitResponse {
        //                 code: 200,
        //                 matchers: vec![],
        //                 post_processors: vec![],
        //                 response_headers: Default::default(),
        //                 json_request_context: false,
        //                 response_template: "Template multiline".to_string(),
        //             }],
        //         },
        //         UriDeceit {
        //             uri: "/some/url".to_string(),
        //             matchers: vec![
        //                 Matcher::Method {
        //                     eq: "POST".to_string(),
        //                 },
        //                 Matcher::Header {
        //                     key: "Content-Type".to_string(),
        //                     value: "application/json".to_string(),
        //                 },
        //             ],
        //             response_headers: Default::default(),
        //             response_code: Some(200),
        //             responses: vec![DeceitResponse {
        //                 code: 200,
        //                 matchers: vec![],
        //                 post_processors: vec![],
        //                 response_headers: Default::default(),
        //                 json_request_context: false,
        //                 response_template: "Template multiline".to_string(),
        //             }],
        //         },
        //     ],
        // };

        // let toml_string = toml::to_string(&specs).unwrap();
        // println!("{toml_string}");
    }

    const TOML_TEST: &str = include_str!("config.toml");

    #[test]
    fn deserialize_toml() {
        let specs: ApateSpecs = toml::from_str(TOML_TEST).unwrap();

        // The coolest debug approach ever
        println!("{specs:#?}");
    }
}
