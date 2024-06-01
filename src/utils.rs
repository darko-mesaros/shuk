use aws_config::environment::credentials::EnvironmentVariableCredentialsProvider;
use aws_config::meta::credentials::CredentialsProviderChain;
use aws_config::meta::region::RegionProviderChain;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_config::BehaviorVersion;
use aws_types::region::Region;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::exit;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use clap::Parser;

use serde::Deserialize;
use serde::Deserializer;

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
pub async fn configure_aws(fallback_region: String, profile_name: String) -> aws_config::SdkConfig {
    let region_provider =
        // NOTE: this is different than the default Rust SDK behavior which checks AWS_REGION first. Is this intentional?
        RegionProviderChain::first_try(env::var("AWS_DEFAULT_REGION").ok().map(Region::new))
            .or_default_provider()
            .or_else(Region::new(fallback_region));

    // NOTE: This checks, ENV first, then profile, then it falls back to the whatever the default
    // is
    let provider = CredentialsProviderChain::first_try(
        "Environment",
        EnvironmentVariableCredentialsProvider::new(),
    )
    .or_else(
        "Profile",
        ProfileFileCredentialsProvider::builder()
            .profile_name(profile_name)
            .build(),
    )
    .or_default_provider()
    .await;

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
#[derive(Deserialize)]
pub struct Config {
    pub bucket_name: String,
    #[serde(deserialize_with = "deserialize_prefix")]
    pub bucket_prefix: Option<String>,
    pub presigned_time: u64,
    pub aws_profile: String,
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
    pub fn load_config(filename: String) -> Result<Self, anyhow::Error> {
        let _contents: String = match fs::read_to_string(filename) {
            Ok(c) => {
                let config: Config = toml::from_str::<Config>(&c).unwrap();
                return Ok(config);
            }
            Err(e) => {
                eprintln!("Could not read config file! {}", e);
                exit(1);
            }
        };
    }
}
//======================================== END CONFIG PARSING
//======================================== ARGUMENT PARSING
#[derive(Parser, Default)]
#[command(version, about, long_about = None)]
pub struct Args {
    pub filename: PathBuf,
}
//======================================== END ARGUMENT PARSING
