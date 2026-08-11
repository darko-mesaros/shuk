use aws_sdk_s3::{
    error::{DisplayErrorContext, ProvideErrorMetadata, SdkError},
    operation::{RequestId, RequestIdExt},
    Client,
};
use std::{error::Error, fmt};

#[derive(Debug)]
pub struct S3OperationError {
    operation: &'static str,
    bucket: String,
    key: Option<String>,
    configured_region: Option<String>,
    reported_region: Option<String>,
    status: Option<u16>,
    code: Option<String>,
    message: Option<String>,
    request_id: Option<String>,
    extended_request_id: Option<String>,
    details: String,
}

impl S3OperationError {
    pub fn from_sdk_error<E>(
        operation: &'static str,
        client: &Client,
        bucket: &str,
        key: Option<&str>,
        error: &SdkError<E>,
    ) -> Self
    where
        E: ProvideErrorMetadata + Error + 'static,
    {
        let response = error.raw_response();
        let configured_region = client
            .config()
            .region()
            .map(|region| region.as_ref().to_string());
        let reported_region = response
            .and_then(|response| response.headers().get("x-amz-bucket-region"))
            .map(str::to_string);

        Self {
            operation,
            bucket: bucket.to_string(),
            key: key.map(str::to_string),
            configured_region,
            reported_region,
            status: response.map(|response| response.status().as_u16()),
            code: error.code().map(str::to_string),
            message: error.message().map(str::to_string),
            request_id: error.request_id().map(str::to_string),
            extended_request_id: error.extended_request_id().map(str::to_string),
            details: format!("{}", DisplayErrorContext(error)),
        }
    }

    pub fn retry_region(&self) -> Option<&str> {
        match (
            self.configured_region.as_deref(),
            self.reported_region.as_deref(),
        ) {
            (Some(configured), Some(reported)) if configured != reported => Some(reported),
            (None, Some(reported)) => Some(reported),
            _ => None,
        }
    }

    pub fn configured_region(&self) -> Option<&str> {
        self.configured_region.as_deref()
    }

    pub fn extended_request_id(&self) -> Option<&str> {
        self.extended_request_id.as_deref()
    }
}

impl fmt::Display for S3OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "S3 {} failed for s3://{}", self.operation, self.bucket)?;
        if let Some(key) = &self.key {
            write!(f, "/{key}")?;
        }
        if let Some(region) = &self.configured_region {
            write!(f, " using region `{region}`")?;
        }

        let mut diagnostics = Vec::new();
        if let Some(status) = self.status {
            diagnostics.push(format!("HTTP {status}"));
        }
        if let Some(code) = &self.code {
            diagnostics.push(match &self.message {
                Some(message) => format!("{code}: {message}"),
                None => code.clone(),
            });
        } else if let Some(message) = &self.message {
            diagnostics.push(message.clone());
        }
        if let Some(request_id) = &self.request_id {
            diagnostics.push(format!("AWS request ID {request_id}"));
        }
        if !diagnostics.is_empty() {
            write!(f, " ({})", diagnostics.join(", "))?;
        }

        if let Some(region) = &self.reported_region {
            if self.configured_region.as_deref() != Some(region) {
                write!(f, ". S3 reports that the bucket is in `{region}`")?;
            }
        }

        if self.status.is_none() && self.code.is_none() && self.message.is_none() {
            write!(f, ". SDK details: {}", self.details)?;
        }

        Ok(())
    }
}

impl Error for S3OperationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::operation::head_object::HeadObjectError;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_types::body::SdkBody;
    use aws_types::region::Region;

    fn client_in(region: &'static str) -> Client {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(Region::new(region))
            .build();
        Client::from_conf(config)
    }

    fn response(status: u16, bucket_region: Option<&str>) -> HttpResponse {
        let mut builder = http::Response::builder()
            .status(status)
            .header("x-amz-request-id", "request-123")
            .header("x-amz-id-2", "extended-456");
        if let Some(region) = bucket_region {
            builder = builder.header("x-amz-bucket-region", region);
        }
        HttpResponse::try_from(builder.body(SdkBody::empty()).unwrap()).unwrap()
    }

    #[test]
    fn extracts_retry_region_and_request_ids() {
        let error = SdkError::service_error(
            HeadObjectError::unhandled(std::io::Error::other("redirect")),
            response(301, Some("us-west-2")),
        );
        let details = S3OperationError::from_sdk_error(
            "HeadObject",
            &client_in("us-east-1"),
            "example-bucket",
            Some("video.mp4"),
            &error,
        );

        assert_eq!(details.configured_region(), Some("us-east-1"));
        assert_eq!(details.retry_region(), Some("us-west-2"));
        assert_eq!(details.extended_request_id(), Some("extended-456"));
        assert_eq!(
            details.to_string(),
            "S3 HeadObject failed for s3://example-bucket/video.mp4 using region `us-east-1` (HTTP 301, AWS request ID request-123). S3 reports that the bucket is in `us-west-2`"
        );
    }

    #[test]
    fn does_not_retry_when_reported_region_matches() {
        let error = SdkError::service_error(
            HeadObjectError::unhandled(std::io::Error::other("access denied")),
            response(403, Some("us-west-2")),
        );
        let details = S3OperationError::from_sdk_error(
            "HeadObject",
            &client_in("us-west-2"),
            "example-bucket",
            Some("video.mp4"),
            &error,
        );

        assert_eq!(details.retry_region(), None);
    }
}
