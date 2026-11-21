//! Feature-flag related code.

use std::sync::Arc;

use unleash_api_client::{
    http::{Method, Request, Response, Transport},
    Client, ClientBuilder,
};
use uuid::Uuid;

/// An Unleash client.
pub type Unleash<F> = Arc<Client<F>>;
pub use unleash_api_client::{client::FeatureKey, Context};

/// An error returned by Unleash.
type UnleashError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Create a new Unleash client, register it, and spawn task to poll for updates.
pub async fn client<F>(
    app_name: &str,
    url: &str,
    secret: String,
) -> Result<Unleash<F>, UnleashError>
where
    F: FeatureKey + Send + Sync,
{
    let instance_id = Uuid::new_v4().to_string();

    let client: Unleash<F> = Arc::new(ClientBuilder::default().into_client_with_transport(
        url,
        app_name,
        &instance_id,
        Some(secret),
        Arc::new(ReqwestTransport(reqwest::Client::new())),
    )?);

    client.register().await?;

    tokio::spawn({
        let client = client.clone();

        async move { client.poll_for_updates().await }
    });

    Ok(client)
}

struct ReqwestTransport(reqwest::Client);

#[async_trait::async_trait]
impl Transport for ReqwestTransport {
    async fn execute(&self, request: Request) -> Result<Response, anyhow::Error> {
        let mut builder = match request.method {
            Method::Get => self.0.get(&request.url),
            Method::Post => self.0.post(&request.url),
        };
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder
                .header("content-type", "application/json")
                .body(body);
        }

        let response = builder.send().await?;
        let status = response.status().as_u16();
        let body = response.bytes().await?.to_vec();

        Ok(Response { status, body })
    }
}
