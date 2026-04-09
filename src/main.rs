pub mod constants;
pub mod file_management;
pub mod infra;
pub mod password;
pub mod upload;
pub mod utils;

use clap::Parser;
use colored::Colorize;
use std::io;
use std::io::Write;
use upload::upload_object;
use utils::check_for_config;
use utils::initialize_config;
use utils::print_warning;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    if let Err(e) = run().await {
        eprintln!("❌ {}", utils::format_aws_error(&e));
        std::process::exit(1);
    }
    Ok(())
}

async fn run() -> Result<(), anyhow::Error> {
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

    // Handle infra commands
    if arguments.deploy_infra {
        infra::deploy_infra().await?;
        std::process::exit(0);
    }
    if arguments.destroy_infra {
        infra::destroy_infra().await?;
        std::process::exit(0);
    }
    if arguments.infra_status {
        infra::infra_status().await?;
        std::process::exit(0);
    }

    // parse configuration
    let shuk_config = utils::Config::load_config()?;
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
            log::warn!("Could not check if file exists: {}", e);
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
            if arguments.password.is_some() {
                // Check if deployed infra bucket matches current config
                if let Some(state) = infra::InfraState::load() {
                    if state.bucket_name != shuk_config.bucket_name {
                        eprintln!("{}", "========================================".yellow());
                        eprintln!("{}", "⚠️  Bucket mismatch detected!".yellow().bold());
                        eprintln!(
                            "  Your shuk.toml uses bucket: {}",
                            shuk_config.bucket_name.cyan()
                        );
                        eprintln!(
                            "  But the deployed infra is scoped to: {}",
                            state.bucket_name.cyan()
                        );
                        eprintln!("  The recipient won't be able to download the file.");
                        eprintln!();
                        eprintln!(
                            "  Run {} to update the infrastructure.",
                            "shuk --deploy-infra".green().bold()
                        );
                        eprintln!("{}", "========================================".yellow());
                        std::process::exit(1);
                    }
                }

                // Password-protected sharing flow
                let pw_text = arguments.password.as_ref().unwrap();
                let pw_hash = password::hash_password(pw_text);
                let share_id = uuid::Uuid::new_v4().to_string();

                // S3 region (where the file lives)
                let s3_region = config
                    .region()
                    .map(|r| r.as_ref().to_string())
                    .unwrap_or_else(|| "us-east-1".to_string());

                // Resolve frontend URL and infra region
                let (frontend_url, infra_region) =
                    password::resolve_frontend_url(&shuk_config, &config).await?;

                // DynamoDB client must target the infra region (where the stack is deployed)
                let infra_config = utils::configure_aws(
                    infra_region.clone(),
                    shuk_config.aws_profile.as_ref(),
                )
                .await;
                let dynamo_client = aws_sdk_dynamodb::Client::new(&infra_config);

                password::create_share(
                    &dynamo_client,
                    &share_id,
                    &pw_hash,
                    &shuk_config.bucket_name,
                    &key_full,
                    shuk_config.presigned_time,
                    &s3_region,
                )
                .await?;

                let share_url = format!("{}/share/{}", frontend_url.trim_end_matches('/'), share_id);

                println!("========================================");
                println!("🔒 | Password-protected share created:");
                println!("🔒 | {}", share_url);

                if shuk_config.use_clipboard.unwrap_or(false) {
                    if let Err(e) = utils::set_into_clipboard(share_url) {
                        eprintln!("Error setting clipboard: {}", e);
                    }
                }
            } else if shuk_config.use_clipboard.unwrap_or(false) {
                if let Err(e) = utils::set_into_clipboard(presigned_url) {
                    eprintln!("Error setting clipboard: {}", e);
                }
            }
        }
        Err(e) => {
            return Err(e);
        }
    }

    Ok(())
}
