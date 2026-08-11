pub mod constants;
pub mod file_management;
pub mod s3_error;
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
        Ok(config) => {
            log::trace!("The configuration is loaded from the file: {:#?}", &config);
            config
        },
        Err(e) => {
            eprintln!("Failed to load configuration. Make sure that your config file is located at ~/.config/shuk: {}", e);
            std::process::exit(1);
        }
    };
    // Configure AWS and create the initial S3 client.
    let config = utils::configure_aws(
        shuk_config
            .fallback_region
            .as_deref()
            .unwrap_or("us-east-1")
            .to_string(),
        shuk_config.aws_profile.as_ref(),
    )
    .await;
    let mut s3_client = aws_sdk_s3::Client::new(&config);

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

    let fail_file_check = |error: &crate::s3_error::S3OperationError| {
        eprintln!(
            "Error: Could not determine whether s3://{}/{} exists.",
            shuk_config.bucket_name, key_full
        );
        eprintln!("Details: {error}");
        eprintln!(
            "Refusing to upload because Shuk could not safely determine whether it would replace an existing object."
        );
        if let Some(request_id) = error.extended_request_id() {
            log::debug!("S3 extended request ID: {request_id}");
        }
        std::process::exit(1);
    };

    let object_exists = match file_management::file_exists_in_s3(
        &s3_client,
        &shuk_config.bucket_name,
        key_full.as_str(),
    )
    .await
    {
        Ok(exists) => exists,
        Err(error) => {
            let Some(bucket_region) = error.retry_region().map(str::to_string) else {
                fail_file_check(&error)
            };
            let configured_region = error.configured_region().unwrap_or("unknown");
            eprintln!(
                "Warning: AWS selected region `{configured_region}`, but bucket `{}` is in `{bucket_region}`.",
                shuk_config.bucket_name
            );
            eprintln!("Retrying automatically in `{bucket_region}`.");
            eprintln!(
                "Tip: Update your AWS region setting or set fallback_region = \"{bucket_region}\" in the Shuk configuration to avoid this extra request."
            );

            s3_client = utils::s3_client_for_region(&config, bucket_region);
            match file_management::file_exists_in_s3(
                &s3_client,
                &shuk_config.bucket_name,
                key_full.as_str(),
            )
            .await
            {
                Ok(exists) => exists,
                Err(retry_error) => fail_file_check(&retry_error),
            }
        }
    };

    let just_upload = if object_exists {
        file_management::quick_compare(
            &file_name,
            &shuk_config.bucket_name,
            key_full.as_str(),
            &file_tags,
            &s3_client,
        )
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "Could not compare the local file with s3://{}/{}: {}",
                shuk_config.bucket_name,
                key_full,
                error
            )
        })?
    } else {
        false
    };

    // Upload-only early exit: file already exists and matches
    if arguments.upload_only && just_upload {
        log::trace!("Upload-only mode: file already exists in S3 and matches, no action needed.");
        println!("========================================");
        println!("✅ | File already exists in S3: {}", key_file_name);
        println!("✅ | No action taken (upload-only mode)");
        println!("========================================");
        std::process::exit(0);
    }

    match upload_object(
        &s3_client,
        &file_name,
        key_file_name,
        file_tags,
        just_upload,
        arguments.upload_only,
        &shuk_config,
    )
    .await
    {
        Ok(Some(presigned_url)) => {
            if shuk_config.use_clipboard.unwrap_or(false) {
                if let Err(e) = utils::set_into_clipboard(presigned_url) {
                    eprintln!("Error setting clipboard: {}", e);
                }
            }
        }
        Ok(None) => {
            // Upload-only mode succeeded — no presigned URL to handle
            log::trace!("Upload-only mode: presigned URL generation was skipped, clipboard operations skipped.");
        }
        Err(e) => {
            eprintln!("Error uploading file: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
