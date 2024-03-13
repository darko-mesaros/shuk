use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use std::time::Duration;

pub async fn presign_file(client: &Client, bucket_name: &str, key: &str, presigned_time: u64) -> Result<String, anyhow::Error>{
    let expires_in = Duration::from_secs(presigned_time);
    let presigned_request = client
        .get_object()
        .bucket(bucket_name)
        .key(key)
        .presigned(PresigningConfig::expires_in(expires_in)?)
        .await?;

    Ok(presigned_request.uri().to_string())
}
