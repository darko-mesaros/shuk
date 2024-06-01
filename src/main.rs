pub mod utils;
pub mod upload;
pub mod file_management;

use upload::upload_object;
use upload::upload_multipart_object;
use tracing::Level;
use clap::Parser;
use std::fs::File;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error>{
    // configure tracing
    utils::configure_tracing(Level::WARN);
    // parse arguments
    let arguments = utils::Args::parse();
    // parse configuration
    let shuk_config = utils::Config::load_config("shuk.toml".to_string())?;
    // configure aws
    let config = utils::configure_aws("us-west-2".into()).await;
    // setup the bedrock-runtime client
    let s3_client = aws_sdk_s3::Client::new(&config);

    let key = arguments.filename.clone();
    let file_name = arguments.filename;

    let file = File::open(&file_name).expect("Failed to open file");
    let metadata = file.metadata().expect("Failed to get file metadata");
    let file_size = metadata.len();

    // If file is bigger than 4GB
    if file_size > 4294967296 {
        upload_multipart_object(&s3_client, &shuk_config.bucket_name, &file_name, key.to_str().unwrap(), shuk_config.presigned_time).await?;
    } else {
        upload_object(&s3_client, &shuk_config.bucket_name, &file_name, key.to_str().unwrap(), shuk_config.presigned_time).await?;
    }

    Ok(())
}
