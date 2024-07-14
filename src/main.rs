pub mod constants;
pub mod file_management;
pub mod upload;
pub mod utils;

use clap::Parser;
use std::io;
use std::io::Write;
use tracing::Level;
use upload::upload_object;
use utils::check_for_config;
use utils::initialize_config;
use utils::print_warning;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // configure tracing
    utils::configure_tracing(Level::ERROR);
    // parse arguments
    let arguments = utils::Args::parse();

    // Checking for the `--init` flag and then initializing the configuration
    if arguments.init {
        if check_for_config() {
            print_warning("****************************************");
            print_warning("WARNING:");
            println!("You are trying to initialize the Shuk configuration");
            println!("This will overwrite your configuration files in $HOME/.config/shuk/");
            print!("ARE YOU SURE YOU WANT DO TO THIS? Y/N: ");
            io::stdout().flush()?; // so the answers are typed on the same line as above

            let mut confirmation = String::new();
            io::stdin().read_line(&mut confirmation)?;
            if confirmation.trim().eq_ignore_ascii_case("y") {
                print_warning("I ask AGAIN");
                print!("ARE YOU SURE YOU WANT DO TO THIS? Y/N: ");
                io::stdout().flush()?; // so the answers are typed on the same line as above

                let mut confirmation = String::new();
                io::stdin().read_line(&mut confirmation)?;

                if confirmation.trim().eq_ignore_ascii_case("y") {
                    println!("----------------------------------------");
                    println!("📜 | Initializing Shuk configuration.");
                    initialize_config().await?;
                }
            }
        } else {
            println!("----------------------------------------");
            println!("📜 | Initializing Shuk configuration.");
            initialize_config().await?;
        }
        print_warning("Shuk will now exit");
        std::process::exit(0);
    }

    // parse configuration
    let shuk_config = utils::Config::load_config()?;
    // configure aws
    let config = utils::configure_aws("us-west-2", shuk_config.aws_profile).await;
    // setup the bedrock-runtime client
    let s3_client = aws_sdk_s3::Client::new(&config);

    let key = arguments.filename.clone();
    let file_name = arguments.filename;

    // upload the object
    upload_object(
        &s3_client,
        &shuk_config.bucket_name,
        &file_name.expect("Filename not provided"),
        shuk_config.bucket_prefix,
        key.expect("Filename not provided")
            .to_string_lossy()
            .to_string()
            .as_str(),
        shuk_config.presigned_time,
        shuk_config.use_clipboard,
    )
    .await?;

    Ok(())
}
