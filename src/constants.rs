// CONFIGURATION FILES
pub static CONFIG_DIR_NAME: &str = "shuk";
pub static CONFIG_FILE_NAME: &str = "shuk.toml";

pub static DEFAULT_OBJECT_TAG: &str = "deployed_by=shuk";

// UPDATED: 2024-04-20
pub static CONFIG_FILE: &str = r#"bucket_name = "foo"
bucket_prefix = "bar"
presigned_time = 86400
aws_profile = "default"
use_clipboard = false
"#;
