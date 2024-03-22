//! Feature-flag related code.

use std::{fmt::Debug, sync::Arc};

use enum_map::EnumArray;
use serde::{de::DeserializeOwned, Serialize};
use unleash_api_client::{client::CachedFeature, Client, ClientBuilder};
use uuid::Uuid;

/// An Unleash client.
pub type Unleash<F> = Arc<Client<F, reqwest::Client>>;
pub use unleash_api_client::Context;

/// An error returned by Unleash.
type UnleashError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Create a new Unleash client, register it, and spawn task to poll for updates.
pub async fn client<F>(
    app_name: &str,
    url: &str,
    secret: String,
) -> Result<Unleash<F>, UnleashError>
where
    F: Clone + Debug + Serialize + DeserializeOwned + EnumArray<CachedFeature> + 'static,
    <F as EnumArray<CachedFeature>>::Array: Send + Sync,
{
    let instance_id = Uuid::new_v4().to_string();

    let client: Unleash<F> = Arc::new(ClientBuilder::default().into_client(
        url,
        app_name,
        &instance_id,
        Some(secret),
    )?);

    client.register().await?;

    tokio::spawn({
        let client = client.clone();

        async move { client.poll_for_updates().await }
    });

    Ok(client)
}
