pub mod constants;
pub mod file_management;
pub mod upload;
pub mod utils;

use clap::Parser;
use std::io;
use std::io::Write;
use upload::upload_object;
use utils::check_for_config;
use utils::initialize_config;
use utils::print_warning;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Configure Logging
    let arguments = utils::Args::parse();
    utils::setup_logging(arguments.verbose);
    log::trace!("Arguments parsed: {:?} ", &arguments);

    // Checking for the `--init` flag and then initializing the configuration
    if arguments.init {
        log::trace!("The --init parameter has been passed");
        if check_for_config() {
            log::trace!("The configuration already exists");
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
            log::trace!("The configuration does not exist");
            println!("----------------------------------------");
            println!("📜 | Initializing Shuk configuration.");
            initialize_config().await?;
        }
        print_warning("Shuk will now exit");
        std::process::exit(0);
    }

    // parse configuration
    let shuk_config = match utils::Config::load_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration. Make sure that your config file is located at ~/.config/shuk: {}", e);
            std::process::exit(1);
        }
    };
    // configure aws
    let config = utils::configure_aws(
        shuk_config
            .fallback_region
            .as_deref()
            .unwrap_or("us-east-1")
            .to_string(),
        shuk_config.aws_profile.as_ref(),
    )
    .await;
    // setup the bedrock-runtime client
    let s3_client = aws_sdk_s3::Client::new(&config);

    let key = arguments.filename.clone();
    let file_name = arguments
        .filename
        .expect("Unable to determine the file name from the command line parameters");
    // NOTE: Getting just the key (file name)
    let key_file_name = key
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(|s| s.trim_matches('"'))
        .ok_or_else(|| anyhow::anyhow!("Invalid filename provided"))?;

    // FIX: This can be cleaner
    let key_full = if shuk_config.bucket_prefix.is_some() {
        format!(
            "{}{}",
            &shuk_config
                .bucket_prefix
                .clone()
                .unwrap_or_else(|| "".into()),
            &key_file_name
        )
    } else {
        key_file_name.to_string()
    };

    // Calculate partial MD5 of the local file
    let md5_of_file = file_management::calculate_partial_hash(&file_name.clone())?;
    // Prep the tags
    let file_tags = file_management::ObjectTags {
        managed_by: "shuk".into(),
        start_hash: md5_of_file.start_hash,
        end_hash: md5_of_file.end_hash,
    };
    log::trace!("File tags defined: {:#?}", &file_tags);

    let just_upload = match file_management::file_exists_in_s3(
        &s3_client,
        &shuk_config.bucket_name,
        key_full.as_str(),
    )
    .await
    {
        // Call was a success
        Ok(o) => {
            log::trace!("The call to check if the file exists has been a success");
            if o {
                // It exists - lets see if it is the same
                if file_management::quick_compare(
                    &file_name,
                    &shuk_config.bucket_name,
                    key_full.as_str(),
                    &file_tags,
                    &s3_client,
                )
                .await?
                {
                    // They are the same - just presing
                    true
                } else {
                    // They are not the same, upload
                    false
                }
            } else {
                // File does not exist
                // Just upload the file
                false
            }
        }
        // The SDK call failed
        Err(e) => {
            eprintln!("Error: Could not determine if a the file exists - {}", e);
            false
        }
    };

    match upload_object(
        &s3_client,
        &file_name,
        key_file_name,
        file_tags,
        just_upload,
        &shuk_config,
    )
    .await
    {
        Ok(presigned_url) => {
            if shuk_config.use_clipboard.unwrap_or(false) {
                if let Err(e) = utils::set_into_clipboard(presigned_url) {
                    eprintln!("Error setting clipboard: {}", e);
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
