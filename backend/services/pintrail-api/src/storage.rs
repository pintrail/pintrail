//! S3-compatible object storage.
//!
//! Media never passes through the API. Phones PUT originals straight to the
//! bucket with a presigned URL, and read processed output the same way
//! (docs/DESIGN.md §2.5) -- which also means native image and video loaders
//! do not need to attach an Authorization header.

use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;

use crate::config::Settings;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Storage {
    client: Client,
    bucket: String,
    presign_ttl: Duration,
}

impl Storage {
    pub async fn connect(settings: &Settings) -> anyhow::Result<Self> {
        let credentials = aws_sdk_s3::config::Credentials::new(
            &settings.s3_access_key_id,
            &settings.s3_secret_access_key,
            None,
            None,
            "pintrail-config",
        );

        let mut builder = aws_sdk_s3::config::Builder::new()
            .region(aws_sdk_s3::config::Region::new(settings.s3_region.clone()))
            .credentials_provider(credentials)
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest());

        if let Some(endpoint) = &settings.s3_endpoint {
            // MinIO serves buckets as path segments rather than subdomains,
            // and a custom endpoint is meaningless without it.
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }

        Ok(Self {
            client: Client::from_conf(builder.build()),
            bucket: settings.s3_bucket.clone(),
            presign_ttl: settings.presign_ttl,
        })
    }

    /// A time-limited URL the client can PUT bytes to.
    ///
    /// `content_type` is signed into the URL, so the upload cannot claim one
    /// type to the API and store another.
    pub async fn presigned_put(&self, key: &str, content_type: &str) -> AppResult<String> {
        let config = PresigningConfig::expires_in(self.presign_ttl)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("presign config: {e}")))?;

        let request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(config)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("presign put {key}: {e}")))?;

        Ok(request.uri().to_string())
    }

    /// A time-limited URL for reading an object.
    pub async fn presigned_get(&self, key: &str) -> AppResult<String> {
        let config = PresigningConfig::expires_in(self.presign_ttl)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("presign config: {e}")))?;

        let request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(config)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("presign get {key}: {e}")))?;

        Ok(request.uri().to_string())
    }

    /// Object size in bytes, or `None` if it is not there.
    ///
    /// This is how the API learns an upload actually happened. Taking the
    /// client's word for it would queue processing jobs against objects that
    /// were never sent.
    pub async fn head(&self, key: &str) -> AppResult<Option<i64>> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => Ok(Some(output.content_length().unwrap_or(0))),
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_not_found() {
                    Ok(None)
                } else {
                    Err(AppError::Internal(anyhow::anyhow!(
                        "head {key}: {service_err}"
                    )))
                }
            }
        }
    }

    pub async fn delete(&self, key: &str) -> AppResult<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("delete {key}: {e}")))?;

        Ok(())
    }

    /// Confirms the bucket is reachable and exists, so a misconfiguration
    /// surfaces at startup rather than on an author's first upload.
    pub async fn check(&self) -> anyhow::Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("bucket {:?} unreachable: {e}", self.bucket))?;

        Ok(())
    }
}
