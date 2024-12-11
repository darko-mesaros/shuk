use aws_config::environment::credentials::EnvironmentVariableCredentialsProvider;
use aws_config::imds::credentials::ImdsCredentialsProvider;
use aws_config::meta::credentials::CredentialsProviderChain;
use aws_config::meta::region::RegionProviderChain;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_config::profile::ProfileFileRegionProvider;
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
//use clipboard_ext::prelude::*;
//use clipboard_ext::x11_fork::ClipboardContext;

//use arboard::Clipboard;

//use copypasta::{ClipboardContext, ClipboardProvider};

use std::process::{Command, Stdio};
use std::env::consts::OS;

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
    let profile = profile_name.map(|s| s.as_str()).unwrap_or("default");
    let region_provider = RegionProviderChain::first_try(
        ProfileFileRegionProvider::builder()
            .profile_name(profile)
            .build(),
    )
    .or_else(aws_config::environment::EnvironmentVariableRegionProvider::new())
    .or_else(aws_config::imds::region::ImdsRegionProvider::builder().build())
    .or_else(Region::new(fallback_region));

    let credentials_provider = CredentialsProviderChain::first_try(
        "Environment",
        EnvironmentVariableCredentialsProvider::new(),
    )
    .or_else(
        "Profile",
        ProfileFileCredentialsProvider::builder()
            .profile_name(profile)
            .build(),
    )
    .or_else("IMDS", ImdsCredentialsProvider::builder().build());

    aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials_provider)
        .region(region_provider)
        .load()
        .await
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
                    let config: Config = toml::from_str::<Config>(&c).unwrap();
                    return Ok(config);
                }
                Err(e) => {
                    eprintln!("Could not read config file! {}", e);
                    eprintln!("Your configuration file needs to be in $HOME/.config/shuk/shuk.toml; Please run the configuration command: shuk --init");
                    exit(1);
                }
            };
        } else {
            eprintln!("Could not read config file!");
            eprintln!("Your configuration file needs to be in $HOME/.config/shuk/shuk.toml; Please run the configuration command: shuk --init");
            exit(1);
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

// Store the prisigned url into clipboard
pub fn set_into_clipboard(s: String) -> Result<(), Box<dyn std::error::Error>> {
    log::trace!("Setting into clipboard: {:?}", &s);
    
    match std::env::consts::OS {
        "linux" => {
            // Try Wayland first
            if let Ok(output) = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .arg(&s)
                .output() {
                if output.status.success() {
                    return Ok(());
                }
            }
            
            // Fall back to X11 using xclip
            let mut child = Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to spawn xclip: {}", e))?;
            
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(s.as_bytes())
                    .map_err(|e| format!("Failed to write to xclip: {}", e))?;
            } else {
                return Err("Failed to open stdin for xclip".into());
            }
            
            child.wait()
                .map_err(|e| format!("Failed to wait for xclip: {}", e))?;
        },
        "macos" => {
            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to spawn pbcopy: {}", e))?;
            
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(s.as_bytes())
                    .map_err(|e| format!("Failed to write to pbcopy: {}", e))?;
            } else {
                return Err("Failed to open stdin for pbcopy".into());
            }
            
            child.wait()
                .map_err(|e| format!("Failed to wait for pbcopy: {}", e))?;
        },
        "windows" => {
            let mut child = Command::new("clip")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to spawn clip: {}", e))?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(s.as_bytes())
                    .map_err(|e| format!("Failed to write to clip: {}", e))?;
            } else {
                return Err("Failed to open stdin for clip".into());
            }

            child.wait()
                .map_err(|e| format!("Failed to wait for clip: {}", e))?;
        },
        os => return Err(format!("Unsupported operating system: {}", os).into())
    }
    
    Ok(())
}

//======================================== ARGUMENT PARSING
#[derive(Debug, Parser, Default)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(required_unless_present("init"))]
    pub filename: Option<PathBuf>,
    // the init flag. So we can copy the config files locally
    #[arg(long, conflicts_with("filename"))]
    pub init: bool,
    #[arg(short, long, help = "Enable verbose logging")]
    pub verbose: bool,
}
//=========================ALPHA=============== END ARGUMENT PARSING
