use std::error::Error;
use std::fmt;

use configuration::Configuration;

mod server;
mod walk;

pub struct ServerConfig {
    pub configuration: Configuration,
    pub name: String,
    pub version: String,
}

#[derive(Debug)]
pub struct ServerError(Box<dyn Error + Send + Sync>);

impl ServerError {
    pub(crate) fn new(error: impl Error + Send + Sync + 'static) -> ServerError {
        ServerError(Box::new(error))
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

pub fn start(config: ServerConfig) -> Result<(), ServerError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServerError::new)?;
    runtime.block_on(server::async_start(config))
}
