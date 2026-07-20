//! S3-compatible object storage, shared by the API and the worker.
//!
//! Media never passes through the API. Phones PUT originals straight to the
//! bucket with a presigned URL and read the same way (docs/DESIGN.md §2.5),
//! which also means native image loaders need no Authorization header. The
//! worker reads originals from here and writes processed output back.
//!
//! Errors are `anyhow` rather than the API's error type so both binaries can
//! depend on this. `AppError` converts from `anyhow::Error`, so API handlers
//! still just use `?`.

use std::time::Duration;

use anyhow::Context;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// `None` for real S3; set for MinIO or another S3-compatible endpoint.
    pub endpoint: Option<String>,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub presign_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct Storage {
    client: Client,
    bucket: String,
    presign_ttl: Duration,
}

impl Storage {
    pub async fn connect(config: &StorageConfig) -> anyhow::Result<Self> {
        let credentials = aws_sdk_s3::config::Credentials::new(
            &config.access_key_id,
            &config.secret_access_key,
            None,
            None,
            "pintrail-config",
        );

        let mut builder = aws_sdk_s3::config::Builder::new()
            .region(aws_sdk_s3::config::Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest());

        if let Some(endpoint) = &config.endpoint {
            // MinIO serves buckets as path segments rather than subdomains, and
            // a custom endpoint is meaningless without it.
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }

        Ok(Self {
            client: Client::from_conf(builder.build()),
            bucket: config.bucket.clone(),
            presign_ttl: config.presign_ttl,
        })
    }

    pub fn presign_ttl(&self) -> Duration {
        self.presign_ttl
    }

    /// A time-limited URL the client can PUT bytes to.
    ///
    /// `content_type` is signed into the URL, so an upload cannot claim one
    /// type to the API and store another.
    pub async fn presigned_put(&self, key: &str, content_type: &str) -> anyhow::Result<String> {
        let config = PresigningConfig::expires_in(self.presign_ttl)?;

        let request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(config)
            .await
            .with_context(|| format!("presign put {key}"))?;

        Ok(request.uri().to_string())
    }

    pub async fn presigned_get(&self, key: &str) -> anyhow::Result<String> {
        let config = PresigningConfig::expires_in(self.presign_ttl)?;

        let request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(config)
            .await
            .with_context(|| format!("presign get {key}"))?;

        Ok(request.uri().to_string())
    }

    /// Object size in bytes, or `None` if it is not there.
    ///
    /// This is how the API learns an upload actually happened; taking the
    /// client's word would queue processing jobs for objects never sent.
    pub async fn head(&self, key: &str) -> anyhow::Result<Option<i64>> {
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
                    Err(anyhow::anyhow!("head {key}: {service_err}"))
                }
            }
        }
    }

    /// Downloads an object in full.
    ///
    /// The worker needs the bytes in memory to decode them; the size limit
    /// enforced at upload completion is what keeps this bounded.
    pub async fn get(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .with_context(|| format!("get {key}"))?;

        let bytes = output
            .body
            .collect()
            .await
            .with_context(|| format!("read body of {key}"))?;

        Ok(bytes.into_bytes().to_vec())
    }

    pub async fn put(&self, key: &str, body: Vec<u8>, content_type: &str) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(body))
            .send()
            .await
            .with_context(|| format!("put {key}"))?;

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .with_context(|| format!("delete {key}"))?;

        Ok(())
    }

    /// Confirms the bucket is reachable, so a misconfiguration surfaces at
    /// startup rather than on an author's first upload.
    pub async fn check(&self) -> anyhow::Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .with_context(|| format!("bucket {:?} unreachable", self.bucket))?;

        Ok(())
    }
}
