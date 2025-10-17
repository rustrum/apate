use std::io;

use apate::{apate_init_config, apate_run};

#[actix_web::main]
async fn main() -> io::Result<()> {
    let config = apate_init_config().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    log::debug!("Configuration initialized: {:?}", config);

    apate_run(config).await
}
