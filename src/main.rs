pub mod constants;
pub mod file_management;
pub mod metadata;
pub mod upload;
pub mod utils;

use clap::Parser;
use std::io;
use std::io::Write;
use upload::upload_object;
use utils::check_for_config;
use utils::initialize_config;
use utils::print_warning;

use file_management::{
    delete_file, presign_file, prompt_for_file_selection, select_action, FileAction,
};
use metadata::{create_metadata, save_metadata, MetadataTags};

use colored::Colorize;

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
        }
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

    // if the filename is being passed - we upload
    if arguments.filename.is_some() {
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

        // Done with the upload, exit
        std::process::exit(0)
    }

    // Nothing being passed let's do filemanagement
    let metadata_tags = MetadataTags{
        managed_by: "shuk".into(),
        // TODO: Maybe make this into an actual bool
        do_not_scan: "true".into(),
    };
    log::trace!("Metadata tags defined: {:#?}", &metadata_tags);
    // Create Metadata
    // FIX: Clean up this prefix nonsense
    let bucket_prefix = &shuk_config.bucket_prefix.unwrap_or_default();
    let mut shuk_metadata =
        create_metadata(&s3_client, &shuk_config.bucket_name, bucket_prefix).await?;

    log::trace!("Attempting to save updated metadata locally");
    if let Err(e) = save_metadata(
        &s3_client,
        &shuk_config.bucket_name,
        bucket_prefix,
        &shuk_metadata,
        &metadata_tags,
    )
    .await
    {
        log::warn!("Failed to save metadata locally: {}", e);
        println!(
            "{}",
            format!("Warning: Failed to save metadata: {}", e).yellow()
        );
    } else {
        log::trace!("Successfully saved metadata locally");
    }

    // Get some statistics
    let total_file_size = shuk_metadata.get_total_file_size_formatted();
    let number_of_files = shuk_metadata.file_count().unwrap_or(0_usize); // if I cannot get the
                                                                         // number of files, just
                                                                         // return a usize of 0
    // Get the last modified file timestamp
    let last_modified_file = match shuk_metadata.get_most_recent_file() {
        Ok(file) => file.last_modified,
        // If we cannot get the time, just return now TODO: deal with this better
        Err(_) => chrono::Utc::now(),
    };

    println!("{}", "Shuk Metadata:".magenta());
    println!("{}{}", "Total file size: ".green(), total_file_size);
    println!("{}{}", "Total files: ".green(), number_of_files);
    println!("{}{}", "Last modified at: ".green(), last_modified_file);
    println!("----------------------------------------");

    loop {
        let selected = prompt_for_file_selection(&shuk_metadata)?;
        match select_action(&selected)? {
            FileAction::PreSign => {
                log::trace!("Presigning file: {}", &selected);
                let url = presign_file(
                    &s3_client,
                    &shuk_config.bucket_name.to_string(),
                    &selected,
                    shuk_config.presigned_time,
                )
                .await?;
                if shuk_config.use_clipboard.unwrap_or(false) {
                    // TODO: Handle clone
                    if let Err(e) = utils::set_into_clipboard(url.clone()) {
                        eprintln!("Error setting clipboard: {}", e);
                    }
                }

                println!("Here is the URL to the newly pre-signed file:");
                println!("{}", url.cyan());
                println!("----------------------------------------");

                // Update metadata after presigning
                log::trace!("Updating metadata after presigning file: {}", &selected);
                match create_metadata(&s3_client, &shuk_config.bucket_name, bucket_prefix).await {
                    Ok(new_metadata) => {
                        log::trace!("Successfully updated metadata after presigning");
                        shuk_metadata = new_metadata;
                        // Optionally save the metadata locally
                        log::trace!("Attempting to save updated metadata locally");
                        if let Err(e) = save_metadata(
                            &s3_client,
                            &shuk_config.bucket_name,
                            bucket_prefix,
                            &shuk_metadata,
                            &metadata_tags,
                        )
                        .await
                        {
                            log::warn!("Failed to save metadata locally: {}", e);
                            println!(
                                "{}",
                                format!("Warning: Failed to save metadata: {}", e).yellow()
                            );
                        } else {
                            log::trace!("Successfully saved metadata locally");
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to update metadata after presigning: {}", e);
                        println!(
                            "{}",
                            format!("Warning: Failed to update metadata: {}", e).yellow()
                        );
                    }
                }
            }
            FileAction::Delete => {
                log::trace!("Deleting file: {}", &selected);
                delete_file(&selected, &shuk_config.bucket_name.to_string(), &s3_client).await?;
                println!("----------------------------------------");

                // Update metadata after deletion
                log::trace!("Updating metadata after deleting file: {}", &selected);
                match create_metadata(&s3_client, &shuk_config.bucket_name, bucket_prefix).await {
                    Ok(new_metadata) => {
                        log::trace!("Successfully updated metadata after deletion");
                        shuk_metadata = new_metadata;
                        // Optionally save the metadata locally
                        log::trace!("Attempting to save updated metadata locally");
                        if let Err(e) = save_metadata(
                            &s3_client,
                            &shuk_config.bucket_name,
                            bucket_prefix,
                            &shuk_metadata,
                            &metadata_tags,
                        )
                        .await
                        {
                            log::warn!("Failed to save metadata locally: {}", e);
                            println!(
                                "{}",
                                format!("Warning: Failed to save metadata: {}", e).yellow()
                            );
                        } else {
                            log::trace!("Successfully saved metadata locally");
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to update metadata after deletion: {}", e);
                        println!(
                            "{}",
                            format!("Warning: Failed to update metadata: {}", e).yellow()
                        );
                    }
                }
            }
        }
    }
}
