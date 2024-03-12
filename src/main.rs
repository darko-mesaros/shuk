pub mod utils;

use shuk::upload_object;
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error>{
    // configure tracing
    utils::configure_tracing(Level::WARN);
    // configure aws
    let config = utils::configure_aws("us-west-2".into()).await;
    // setup the bedrock-runtime client
    let s3_client = aws_sdk_s3::Client::new(&config);

    let bucket_name = "aws-darko-assets";
    let file_name = "testfile";
    let key = "testkey";

    upload_object(&s3_client, bucket_name, file_name, key).await?;

    Ok(())
}
