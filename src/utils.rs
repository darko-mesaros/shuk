use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_types::region::Region;

use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;

use clap::Parser;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;

use crate::constants;
use colored::*;
use dirs::home_dir;

use chrono;

use std::process::{Command, Stdio};

// Configure logging
pub fn setup_logging(verbose: bool) {
    let env =
        env_logger::Env::default().filter_or("SHUK_LOG", if verbose { "trace" } else { "warn" });

    // TODO: Need to add some color here
    env_logger::Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();
}

//======================================== AWS
pub async fn configure_aws(
    fallback_region: String,
    profile_name: Option<&String>,
) -> aws_config::SdkConfig {
    let mut loader = aws_config::defaults(BehaviorVersion::latest());

    if let Some(profile) = profile_name.map(|s| s.as_str()) {
        loader = loader.profile_name(profile);
    } else {
        let region_provider = RegionProviderChain::first_try(
            aws_config::environment::EnvironmentVariableRegionProvider::new(),
        )
        .or_else(aws_config::imds::region::ImdsRegionProvider::builder().build())
        .or_else(Region::new(fallback_region));
        loader = loader.region(region_provider);
    }

    loader.load().await
}

//======================================== END AWS
//======================================== CONFIG PARSING
//NOTE:
//Thank you maroider_ <3
//When the user runs the application it should look for the config file.
//if the file does not exist in `$HOME/.config/shuk/` inform the user,
//tell them to run `shuk --init` and then just ask for the bucketname.
//For the `--init` option, create the configuration file in the users
//`.config` directory from a `CONST` right here in the code.
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub bucket_name: String,
    #[serde(deserialize_with = "deserialize_prefix")]
    pub bucket_prefix: Option<String>,
    pub presigned_time: u64,
    pub aws_profile: Option<String>,
    pub use_clipboard: Option<bool>,
    pub fallback_region: Option<String>,
    pub password_frontend_url: Option<String>,
}

// This function exists so we can append "/" to any prefix we read from the configuration file.
fn deserialize_prefix<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut prefix = String::deserialize(deserializer)?;
    // if its just "" then return none
    if prefix.is_empty() {
        Ok(None)
    } else if prefix.ends_with('/') {
        // check if by some chance we already have '/'
        Ok(Some(prefix))
    } else {
        // append '/' to the prefix
        prefix = format!("{}/", prefix);
        Ok(Some(prefix))
    }
}

impl Config {
    pub fn load_config() -> Result<Self, anyhow::Error> {
        log::trace!("Parsing the configuration file");
        let home_dir = home_dir().expect("Failed to get HOME directory");
        log::trace!("Home directory: {:?}", &home_dir);
        let config_dir = home_dir.join(".config/shuk");
        log::trace!("Config directory: {:?}", &config_dir);
        let config_file_path = config_dir.join(constants::CONFIG_FILE_NAME);
        log::trace!("Config file path: {:?}", &config_file_path);

        if check_for_config() {
            let _contents: String = match fs::read_to_string(config_file_path) {
                Ok(c) => {
                    let config: Config = toml::from_str::<Config>(&c)?;
                    return Ok(config);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Could not read config file: {}", e));
                }
            };
        } else {
            return Err(anyhow::anyhow!("Config file not found. Run 'shuk --init' to create it."));
        }
    }
}
//======================================== END CONFIG PARSING
//
pub fn check_for_config() -> bool {
    log::trace!("Checking for the configuration file");
    let home_dir = home_dir().expect("Failed to get HOME directory");
    log::trace!("Home directory: {:?}", &home_dir);
    let config_dir = home_dir.join(".config/shuk");
    log::trace!("Config directory: {:?}", &config_dir);
    let config_file_path = config_dir.join(constants::CONFIG_FILE_NAME);
    log::trace!("Config file path: {:?}", &config_file_path);

    // returns true or false
    match config_file_path.try_exists() {
        Ok(b) => {
            log::trace!("Config file path: {:?} exists", &config_file_path);
            b
        }
        Err(e) => {
            log::warn!(
                "I was unable to determine if the config file path: {:?} exists",
                &config_file_path
            );
            eprintln!("Was unable to determine if the config file exists: {}", e);
            exit(1);
        }
    }
}

// function that creates the configuration files during the `init` command
pub async fn initialize_config() -> Result<(), anyhow::Error> {
    log::trace!("Initializing the configuration");
    let home_dir = home_dir().expect("Failed to get HOME directory");
    log::trace!("Home directory: {:?}", &home_dir);
    let config_dir = home_dir.join(format!(".config/{}", constants::CONFIG_DIR_NAME));
    log::trace!("Config directory: {:?}", &config_dir);
    log::trace!("Creating the config directory: {:?}", &config_dir);
    fs::create_dir_all(&config_dir)?;

    let config_file_path = config_dir.join(constants::CONFIG_FILE_NAME);
    log::trace!("Config file path: {:?}", &config_file_path);
    let config_content = constants::CONFIG_FILE.to_string();
    log::trace!("Config file contents: {:?}", &config_content);

    log::trace!("Parsing default config into TOML");
    let mut default_config: Config =
        toml::from_str::<Config>(&config_content).expect("default config must be valid");

    // Prompt the user for details
    let mut bucket_name = String::new();
    print!("Enter the name of the bucket you wish to use for file uploads: ");
    io::stdout().flush()?; // so the answers are typed on the same line as above
    io::stdin().read_line(&mut bucket_name)?;
    default_config.bucket_name = bucket_name.trim().to_string();
    log::trace!("Using bucket name: {}", &default_config.bucket_name);

    let mut bucket_prefix = String::new();
    print!("Enter the prefix (folder) in that bucket where the files will be uploaded (leave blank for the root of the bucket): ");
    io::stdout().flush()?; // so the answers are typed on the same line as above
    io::stdin().read_line(&mut bucket_prefix)?;
    default_config.bucket_prefix = Some(bucket_prefix.trim().to_string());
    log::trace!("Using bucket prefix: {:?}", &default_config.bucket_prefix);

    let mut config_profile = String::new();
    print!("Enter the AWS profile name (enter for None): ");
    io::stdout().flush()?; // so the answers are typed on the same line as above
    io::stdin().read_line(&mut config_profile)?;
    let config_profile = config_profile.trim();
    default_config.aws_profile = if config_profile.is_empty() {
        None
    } else {
        Some(config_profile.to_string())
    };
    log::trace!("Using profile : {:?}", &default_config.aws_profile);

    fs::write(&config_file_path, toml::to_string_pretty(&default_config)?)?;
    println!(
        "⏳| Shuk configuration file created at: {:?}",
        config_file_path
    );
    println!("This file is used to store configuration items for the shuk application.");

    println!("✅ | Shuk configuration has been initialized in ~/.config/shuk. You may now use it as normal.");
    Ok(())
}

pub fn print_warning(s: &str) {
    println!("{}", s.yellow());
}

/// Formats errors into user-friendly messages
pub fn format_aws_error(e: &dyn std::fmt::Display) -> String {
    let msg = e.to_string();
    if msg.contains("credentials") || msg.contains("InvalidConfiguration") || msg.contains("credentials-login") {
        "AWS credentials error. Please check your credentials:\n  • Run 'aws configure' to set up credentials\n  • Or set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY environment variables\n  • Or ensure your AWS profile is correctly configured in shuk.toml".to_string()
    } else if msg.contains("ExpiredToken") || msg.contains("expired") || msg.contains("security token") {
        "AWS session has expired. Please re-authenticate:\n  • Run 'aws sso login' if using SSO\n  • Or refresh your credentials".to_string()
    } else if msg.contains("dispatch failure") || msg.contains("DispatchFailure") {
        "Could not connect to AWS. Please check your credentials and network connection:\n  • Run 'aws configure' to set up credentials\n  • Or set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY environment variables\n  • Or ensure your AWS profile is correctly configured in shuk.toml".to_string()
    } else if msg.contains("NoSuchBucket") {
        "S3 bucket not found. Check 'bucket_name' in ~/.config/shuk/shuk.toml".to_string()
    } else if msg.contains("NoSuchKey") {
        "File not found in S3. It may have been deleted.".to_string()
    } else if msg.contains("AccessDenied") || msg.contains("Forbidden") || msg.contains("Access Denied") {
        "Access denied. Your AWS credentials don't have permission for this operation.".to_string()
    } else if msg.contains("Could not find CloudFormation stack") {
        format!("Password-sharing infrastructure not deployed.\n  Run 'shuk --deploy-infra' to set it up.")
    } else if msg.contains("config") && (msg.contains("read") || msg.contains("parse") || msg.contains("not found") || msg.contains("No such file")) {
        "Could not load shuk configuration.\n  Run 'shuk --init' to create your config file at ~/.config/shuk/shuk.toml".to_string()
    } else if msg.contains("ResourceNotFoundException") || msg.contains("Table") && msg.contains("not found") {
        "DynamoDB table not found. The infrastructure may not be deployed.\n  Run 'shuk --deploy-infra' to set it up.".to_string()
    } else {
        msg
    }
}

// Store the prisigned url into clipboard
pub fn set_into_clipboard(s: String) -> Result<(), Box<dyn std::error::Error>> {
    log::trace!("Attempting to set clipboard content");
    log::debug!("Content length to be copied: {}", s.len());

    match std::env::consts::OS {
        "linux" => {
            log::trace!("Detected Linux OS, attempting clipboard operations");

            // Try Wayland first
            log::debug!("Attempting Wayland clipboard (wl-copy)");
            if let Ok(output) = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .arg(&s)
                .output()
            {
                if output.status.success() {
                    log::debug!("Successfully copied to Wayland clipboard");
                    return Ok(());
                }
                log::debug!("Wayland clipboard attempt failed, falling back to X11");
            } else {
                log::debug!("wl-copy not available, falling back to X11");
            }

            // Fall back to X11 using xclip
            log::debug!("Attempting X11 clipboard (xclip)");
            let mut child = Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    log::error!("Failed to spawn xclip: {}", e);
                    format!("Failed to spawn xclip (is it installed?): {}", e)
                })?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(s.as_bytes()).map_err(|e| {
                    log::error!("Failed to write to xclip stdin: {}", e);
                    format!("Failed to write to xclip: {}", e)
                })?;
            } else {
                log::error!("Failed to open stdin for xclip");
                return Err("Failed to open stdin for xclip".into());
            }

            let status = child.wait().map_err(|e| {
                log::error!("Failed to wait for xclip process: {}", e);
                format!("Failed to wait for xclip: {}", e)
            })?;

            if !status.success() {
                log::error!("xclip process failed with status: {}", status);
                return Err(format!("xclip failed with status: {}", status).into());
            }

            log::debug!("Successfully copied to X11 clipboard");
        },
        "macos" => {
            log::trace!("Detected macOS, attempting clipboard operation with pbcopy");
            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    log::error!("Failed to spawn pbcopy: {}", e);
                    format!("Failed to spawn pbcopy: {}", e)
                })?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(s.as_bytes()).map_err(|e| {
                    log::error!("Failed to write to pbcopy stdin: {}", e);
                    format!("Failed to write to pbcopy: {}", e)
                })?;
            } else {
                log::error!("Failed to open stdin for pbcopy");
                return Err("Failed to open stdin for pbcopy".into());
            }

            let status = child.wait().map_err(|e| {
                log::error!("Failed to wait for pbcopy process: {}", e);
                format!("Failed to wait for pbcopy: {}", e)
            })?;

            if !status.success() {
                log::error!("pbcopy process failed with status: {}", status);
                return Err(format!("pbcopy failed with status: {}", status).into());
            }

            log::debug!("Successfully copied to macOS clipboard");
        },
        "windows" => {
            log::trace!("Detected Windows, attempting clipboard operation with clip.exe");
            let mut child = Command::new("clip")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    log::error!("Failed to spawn clip.exe: {}", e);
                    format!("Failed to spawn clip.exe: {}", e)
                })?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(s.as_bytes()).map_err(|e| {
                    log::error!("Failed to write to clip.exe stdin: {}", e);
                    format!("Failed to write to clip.exe: {}", e)
                })?;
            } else {
                log::error!("Failed to open stdin for clip.exe");
                return Err("Failed to open stdin for clip.exe".into());
            }

            let status = child.wait().map_err(|e| {
                log::error!("Failed to wait for clip.exe process: {}", e);
                format!("Failed to wait for clip.exe: {}", e)
            })?;

            if !status.success() {
                log::error!("clip.exe process failed with status: {}", status);
                return Err(format!("clip.exe failed with status: {}", status).into());
            }

            log::debug!("Successfully copied to Windows clipboard");
        },
        os => {
            log::error!("Unsupported operating system: {}", os);
            return Err(format!("Unsupported operating system: {}", os).into());
        }
    }

    log::trace!("Clipboard operation completed successfully");
    Ok(())
}

//======================================== ARGUMENT PARSING
#[derive(Debug, Parser, Default)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(required_unless_present_any(["init", "deploy_infra", "destroy_infra", "infra_status"]))]
    pub filename: Option<PathBuf>,
    // the init flag. So we can copy the config files locally
    #[arg(long, conflicts_with("filename"))]
    pub init: bool,
    #[arg(short, long, help = "Enable verbose logging")]
    pub verbose: bool,
    #[arg(long, help = "Password-protect the shared file")]
    pub password: Option<String>,
    #[arg(long, help = "Deploy the password-sharing infrastructure")]
    pub deploy_infra: bool,
    #[arg(long, help = "Destroy the password-sharing infrastructure")]
    pub destroy_infra: bool,
    #[arg(long, help = "Check the status of deployed infrastructure")]
    pub infra_status: bool,
}
//=========================ALPHA=============== END ARGUMENT PARSING
