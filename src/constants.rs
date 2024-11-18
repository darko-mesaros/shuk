// CONFIGURATION FILES
pub static CONFIG_DIR_NAME: &str = "shuk";
pub static CONFIG_FILE_NAME: &str = "shuk.toml";

// UPDATED: 2024-04-20
pub static CONFIG_FILE: &str = r#"bucket_name = "foo"
bucket_prefix = "bar"
presigned_time = 86400
aws_profile = "default"
use_clipboard = false
fallback_region = "us-east-1"
"#;
