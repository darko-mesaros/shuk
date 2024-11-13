use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use std::{path::Path, time::Duration};
use std::fs::File;
use std::io::Read;

pub async fn presign_file(
    client: &Client,
    bucket_name: &str,
    key: &str,
    prefix: Option<String>,
    presigned_time: u64,
) -> Result<String, anyhow::Error> {
    let expires_in = Duration::from_secs(presigned_time);
    let presigned_request = client
        .get_object()
        .bucket(bucket_name)
        .key(format!("{}{}", prefix.unwrap_or("".into()), key))
        .presigned(PresigningConfig::expires_in(expires_in)?)
        .await?;

    Ok(presigned_request.uri().to_string())
}

pub fn calculate_file_md5<P: AsRef<Path>>(path: P) -> Result<String, anyhow::Error> {
    // Open and read the entire file
    let mut file = File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    
    // Calculate MD5
    let digest = md5::compute(&contents);
    
    // Convert to hex string
    Ok(format!("{:x}", digest))
}
