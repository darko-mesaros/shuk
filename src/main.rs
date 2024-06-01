pub mod constants;
pub mod file_management;
pub mod upload;
pub mod utils;

use clap::Parser;
use tracing::Level;
use upload::upload_object;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // configure tracing
    utils::configure_tracing(Level::WARN);
    // parse arguments
    let arguments = utils::Args::parse();
    // parse configuration
    let shuk_config = utils::Config::load_config("shuk.toml".to_string())?;
    // configure aws
    let config = utils::configure_aws("us-west-2".into(), shuk_config.aws_profile).await;
    // setup the bedrock-runtime client
    let s3_client = aws_sdk_s3::Client::new(&config);

    let key = arguments.filename.clone();
    let file_name = arguments.filename;

    // upload the object
    upload_object(
        &s3_client,
        &shuk_config.bucket_name,
        &file_name,
        shuk_config.bucket_prefix,
        key.to_string_lossy().to_string().as_str(),
        shuk_config.presigned_time,
    )
    .await?;

    Ok(())
}
