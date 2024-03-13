use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_types::region::Region;

use std::env;
use std::path::PathBuf;
use std::fs;
use std::process::exit;
use tracing_subscriber::FmtSubscriber;
use tracing::Level;

use clap::Parser;

use serde_derive::Deserialize;

//======================================== TRACING
pub fn configure_tracing(level: Level) {
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(level)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");
}
//======================================== END TRACING

//======================================== AWS
pub async fn configure_aws(s: String) -> aws_config::SdkConfig {
    let provider = RegionProviderChain::first_try(env::var("AWS_DEFAULT_REGION").ok().map(Region::new))
        .or_default_provider()
        .or_else(Region::new(s));

    aws_config::defaults(BehaviorVersion::latest())
        .region(provider)
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
    pub presigned_time: u64,
}

impl Config {
    pub fn load_config(filename: String) -> Result<Self, anyhow::Error> {
        let _contents: String = match fs::read_to_string(filename) {
            Ok(c) => {
                let config: Config = toml::from_str::<Config>(&c).unwrap();
                return Ok(config);

            }
            Err(e) => {
                eprintln!("Could not read config file! {}",e);
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
    pub filename: PathBuf
}
//======================================== END ARGUMENT PARSING
