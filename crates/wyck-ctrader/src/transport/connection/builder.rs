//! Building a [`Client`] in one step.

use crate::config::{ClientCredentials, ConnectionConfig, Environment};
use crate::error::Result;
use crate::transport::connection::Client;

/// Builds a [`Client`]: the friendliest entry point for a newcomer, connecting and (optionally)
/// identifying the application in one call. Skip it and call [`Client::connect`] directly when the
/// two steps are better kept apart (for example to report progress between them).
///
/// ```no_run
/// use wyck_ctrader::{ClientBuilder, ClientCredentials, Environment};
///
/// # async fn demo() -> wyck_ctrader::Result<()> {
/// let client = ClientBuilder::new(Environment::Demo)
///     .credentials(ClientCredentials::new("my-client-id", "my-client-secret"))
///     .connect()
///     .await?;
/// # let _ = client; Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    config: ConnectionConfig,
    credentials: Option<ClientCredentials>,
}

impl ClientBuilder {
    /// A builder with the default settings for `environment`.
    #[must_use]
    pub fn new(environment: Environment) -> Self {
        Self {
            config: ConnectionConfig::new(environment),
            credentials: None,
        }
    }

    /// Identifies the application once connected. Without this, [`ClientBuilder::connect`] only
    /// opens the socket, same as [`Client::connect`].
    #[must_use]
    pub fn credentials(mut self, credentials: ClientCredentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// Connects, and identifies the application when [`ClientBuilder::credentials`] were given.
    ///
    /// # Errors
    ///
    /// Whatever [`Client::connect`] and [`Client::authenticate_application`] return.
    pub async fn connect(self) -> Result<Client> {
        let client = Client::connect(&self.config).await?;
        if let Some(credentials) = &self.credentials {
            client.authenticate_application(credentials).await?;
        }
        Ok(client)
    }
}
