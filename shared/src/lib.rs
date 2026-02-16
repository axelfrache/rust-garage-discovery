use anyhow::{Context, Result};
use aws_config::meta::region::RegionProviderChain;
use aws_credential_types::Credentials;
use aws_sdk_s3::{config::Region, Client};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub last_seen: DateTime<Utc>,
}

pub struct GarageClient {
    client: Client,
    bucket: String,
}

impl GarageClient {
    pub async fn new(endpoint_url: String, bucket: String, region: String) -> Self {
        let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));

        let access_key = std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default();
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_default();

        let credentials_provider = Credentials::new(
            access_key,
            secret_key,
            None,
            None,
            "environment",
        );

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region_provider)
            .endpoint_url(endpoint_url)
            .credentials_provider(credentials_provider)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        let client = Client::from_conf(s3_config);

        Self { client, bucket }
    }

    pub async fn ensure_bucket_exists(&self) -> Result<()> {
        let resp = self.client.list_buckets().send().await;

        let buckets = resp
            .context("Failed to list buckets")?
            .buckets
            .unwrap_or_default();

        if !buckets
            .iter()
            .any(|b| b.name().map_or(false, |n| n == self.bucket))
        {
            self.client
                .create_bucket()
                .bucket(&self.bucket)
                .send()
                .await
                .context("Failed to create bucket")?;
        }

        Ok(())
    }

    pub async fn register_service(&self, service: &ServiceRegistration) -> Result<()> {
        let key = format!("receivers/{}.json", service.id);
        let data = serde_json::to_vec(service)?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(data.into())
            .send()
            .await
            .context("Failed to upload service registration")?;

        Ok(())
    }

    pub async fn discover_services(&self) -> Result<Vec<ServiceRegistration>> {
        let mut services = Vec::new();

        let objects = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix("receivers/")
            .send()
            .await
            .context("Failed to list objects in bucket")?;

        for obj in objects.contents() {
            if let Some(key) = obj.key() {
                if key.ends_with('/') {
                    continue;
                }

                let resp = self
                    .client
                    .get_object()
                    .bucket(&self.bucket)
                    .key(key)
                    .send()
                    .await;

                match resp {
                    Ok(output) => {
                        let data = output
                            .body
                            .collect()
                            .await
                            .map(|b| b.into_bytes())
                            .unwrap_or_default();
                        if let Ok(service) = serde_json::from_slice::<ServiceRegistration>(&data) {
                            services.push(service);
                        } else {
                        }
                    }
                    Err(_) => {
                    }
                }
            }
        }

        Ok(services)
    }
}
