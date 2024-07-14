use aws_config::environment::credentials::EnvironmentVariableCredentialsProvider;
use aws_config::meta::credentials::CredentialsProviderChain;
use aws_config::meta::region::RegionProviderChain;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_config::BehaviorVersion;
use aws_types::region::Region;

use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use clap::Parser;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;

use crate::constants;
use colored::*;
use dirs::home_dir;

use clipboard::ClipboardProvider;
use clipboard::ClipboardContext;

//======================================== TRACING
pub fn configure_tracing(level: Level) {
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(level)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
//======================================== END TRACING
//======================================== AWS
pub async fn configure_aws(fallback_region: &str, profile_name: Option<String>) -> aws_config::SdkConfig {
    // FIX: 
    // This does not really work on different regions other than us-west-2 (or the region provided
    // in the main.rs)
    let region_provider =
        // NOTE: this is different than the default Rust SDK behavior which checks AWS_REGION first. Is this intentional?
        RegionProviderChain::first_try(env::var("AWS_DEFAULT_REGION").ok().map(Region::new))
            .or_default_provider()
            .or_else(Region::new(fallback_region.to_string()));


    // NOTE: This checks, ENV first, then profile, then it falls back to the whatever the default
    // is

    // Try this first
    let mut provider = CredentialsProviderChain::first_try(
        "Environment",
        EnvironmentVariableCredentialsProvider::new(),
    );

    // if profile_name is set (if it is not None)
    if let Some(profile_name) = profile_name {
        provider = provider
            .or_else( // if the profile_name is empty it still configures it as such
                "Profile",
                ProfileFileCredentialsProvider::builder()
                    .profile_name(profile_name) // what if the profile_name is empty?
                    .build(),
            );

    };

    let provider = provider.or_default_provider().await;

    aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(provider)
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
        let home_dir = home_dir().expect("Failed to get HOME directory");
        let config_dir = home_dir.join(".config/shuk");
        let config_file_path = config_dir.join(constants::CONFIG_FILE_NAME);

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
    let home_dir = home_dir().expect("Failed to get HOME directory");
    let config_dir = home_dir.join(".config/shuk");
    let config_file_path = config_dir.join("shuk.toml");

    // returns true or false
    match config_file_path.try_exists() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Was unable to determine if the config file exists: {}", e);
            exit(1);
        }
    }
}

// function that creates the configuration files during the `init` command
pub async fn initialize_config() -> Result<(), anyhow::Error> {
    let home_dir = home_dir().expect("Failed to get HOME directory");
    let config_dir = home_dir.join(format!(".config/{}", constants::CONFIG_DIR_NAME));
    fs::create_dir_all(&config_dir)?;

    let config_file_path = config_dir.join(constants::CONFIG_FILE_NAME);
    let config_content = constants::CONFIG_FILE.to_string();

    let mut default_config: Config =
        toml::from_str::<Config>(&config_content).expect("default config must be valid");

    // Prompt the user for details
    let mut bucket_name = String::new();
    print!("Enter the name of the bucket you wish to use for file uploads: ");
    io::stdout().flush()?; // so the answers are typed on the same line as above
    io::stdin().read_line(&mut bucket_name)?;
    default_config.bucket_name = bucket_name.trim().to_string();

    let mut bucket_prefix = String::new();
    print!("Enter the prefix (folder) in that bucket where the files will be uploaded (leave blank for the root of the bucket): ");
    io::stdout().flush()?; // so the answers are typed on the same line as above
    io::stdin().read_line(&mut bucket_prefix)?;
    default_config.bucket_prefix = Some(bucket_prefix.trim().to_string());

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
pub fn set_into_clipboard(s: String) {
    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
    ctx.set_contents(s.to_owned()).unwrap();
}

//======================================== ARGUMENT PARSING
#[derive(Parser, Default)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(required_unless_present("init"))]
    pub filename: Option<PathBuf>,
    // the init flag. So we can copy the config files locally
    #[arg(long, conflicts_with("filename"))]
    pub init: bool,
    //pub filename: Option<PathBuf>,
}
//=========================ALPHA=============== END ARGUMENT PARSING
