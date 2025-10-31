use std::io;

use apate::{apate_run, apate_server_init_config};

#[actix_web::main]
async fn main() -> io::Result<()> {
    let (port, log, spec_files) = parse_args()?;

    let config = apate_server_init_config(port, log, spec_files).map_err(io::Error::other)?;

    log::debug!("Configuration initialized: {:?}", config);

    apate_run(config).await
}

fn parse_args() -> io::Result<(Option<u16>, Option<String>, Vec<String>)> {
    let mut port = None;
    let mut log = None;
    let mut files = Vec::new();

    let cli = getopt3::new(getopt3::hideBin(std::env::args()), "p:l:");
    match cli {
        Ok(g) => {
            if let Some(port_str) = g.options.get(&'p') {
                let port_num = port_str
                    .parse::<u16>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                port = Some(port_num)
            }

            if let Some(log_str) = g.options.get(&'l') {
                log = Some(log_str.clone())
            }

            for path in g.arguments {
                files.push(path);
            }

            Ok((port, log, files))
        }
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidInput, e)),
    }
}
