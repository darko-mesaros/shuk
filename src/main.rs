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
    // let shuk_config = utils::Config::load_config()?;
    let shuk_config = match utils::Config::load_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration. Make sure that your config file is located at ~/.config/shuk: {}", e);
            std::process::exit(1);
        }
    };
    // configure aws
    let config = utils::configure_aws("us-west-2", shuk_config.aws_profile).await;
    // setup the bedrock-runtime client
    let s3_client = aws_sdk_s3::Client::new(&config);

    let key = arguments.filename.clone();
    let file_name = arguments.filename;

    // FIX: This can be cleaner
    let key_full = if shuk_config.bucket_prefix.is_some() {
        format!("{}/{:?}",
            &shuk_config.bucket_prefix
                .clone()
                .unwrap_or_else(||"".into()),
            &file_name.clone().unwrap())
    } else {
        format!("{:?}",&file_name.clone().unwrap())
    };

    // Calculate partial MD5 of the local file
    let md5_of_file = file_management::calculate_partial_hash(&file_name.clone().unwrap())?;
    // Prep the tags
    let file_tags = file_management::ObjectTags{
        managed_by: "shuk".into(),
        start_hash: md5_of_file.start_hash,
        end_hash: md5_of_file.end_hash,
    };

    match file_management::file_exists_in_s3(
        &s3_client,
        &shuk_config.bucket_name,
        key_full.as_str()
        ).await {
        // Call was a success
        Ok(o) => if o {
            // File exists
            // Get file metadata
            let object_metadata = file_management::get_file_metadata(&s3_client, &shuk_config.bucket_name, key_full.as_str()).await?;
            let object_tags = file_management::get_file_tags(&s3_client, &shuk_config.bucket_name, key_full.as_str()).await?;
            // Compare the file size
            // If Same
            //   Compare partial hash
            //   If the same - presign
            //   else, upload
            // else, upload
             
        } else {
            // File does not exist
            // Just upload the file

        },
        // The SDK call failed 
        Err(e) => eprintln!("Error: Could not determine if a the file exists - {}", e)
    }

    // upload the object
    match upload_object(
        &s3_client,
        &shuk_config.bucket_name,
        &file_name.expect("Filename not provided"),
        shuk_config.bucket_prefix,
        key.expect("Filename not provided")
            .to_string_lossy()
            .to_string()
            .as_str(),
        shuk_config.presigned_time,
        file_tags,
    )
    .await
    {
        Ok(presigned_url) => {
            if shuk_config.use_clipboard.unwrap_or(false) {
                if let Err(e) = utils::set_into_clipboard(presigned_url) {
                    eprintln!("Error setting clipboard: {}",e);
                }
            }
        }
        Err(e) => {
            eprintln!("Error uploading file: {}", e);
            std::process::exit(1);
        }

    }

    Ok(())
}
