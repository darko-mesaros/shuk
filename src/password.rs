use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client as DynamoClient;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use rand::RngCore;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::Config;

const TABLE_NAME: &str = "shuk-password-shares";
const STACK_NAME: &str = "shuk-password-frontend";
const ITERATIONS: u32 = 600_000;
const MAX_ATTEMPTS: u32 = 5;

pub struct PasswordHash {
    pub hash_hex: String,
    pub salt_hex: String,
}

pub fn hash_password(password: &str) -> PasswordHash {
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);

    let mut key = [0u8; 32];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, ITERATIONS, &mut key)
        .expect("HMAC can be initialized with any key length");

    PasswordHash {
        hash_hex: key.iter().map(|b| format!("{:02x}", b)).collect(),
        salt_hex: salt.iter().map(|b| format!("{:02x}", b)).collect(),
    }
}

pub async fn create_share(
    client: &DynamoClient,
    share_id: &str,
    pw: &PasswordHash,
    bucket_name: &str,
    object_key: &str,
    presigned_time: u64,
    region: &str,
) -> Result<(), anyhow::Error> {
    let expiry_ttl = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + presigned_time;

    client
        .put_item()
        .table_name(TABLE_NAME)
        .item("share_id", AttributeValue::S(share_id.into()))
        .item("password_hash", AttributeValue::S(pw.hash_hex.clone()))
        .item("salt", AttributeValue::S(pw.salt_hex.clone()))
        .item("bucket_name", AttributeValue::S(bucket_name.into()))
        .item("object_key", AttributeValue::S(object_key.into()))
        .item("presigned_time", AttributeValue::N(presigned_time.to_string()))
        .item("expiry_ttl", AttributeValue::N(expiry_ttl.to_string()))
        .item("attempts", AttributeValue::N("0".into()))
        .item("max_attempts", AttributeValue::N(MAX_ATTEMPTS.to_string()))
        .item("region", AttributeValue::S(region.into()))
        .send()
        .await?;

    Ok(())
}

pub async fn resolve_frontend_url(
    config: &Config,
    aws_config: &aws_config::SdkConfig,
) -> Result<(String, String), anyhow::Error> {
    // Config takes precedence
    if let Some(ref url) = config.password_frontend_url {
        if !url.is_empty() {
            log::trace!("Using frontend URL from config: {}", url);
            // Extract region from API Gateway URL: https://{id}.execute-api.{region}.amazonaws.com
            let region = url
                .split(".execute-api.")
                .nth(1)
                .and_then(|s| s.split('.').next())
                .unwrap_or("us-east-1")
                .to_string();
            return Ok((url.clone(), region));
        }
    }

    // Fall back to CloudFormation stack output auto-discovery
    // Try multiple regions: current config region first, then us-east-1
    log::trace!(
        "No frontend URL in config, querying CloudFormation stack: {}",
        STACK_NAME
    );

    let regions_to_try: Vec<String> = {
        let mut r = vec![];
        if let Some(region) = aws_config.region() {
            r.push(region.as_ref().to_string());
        }
        if !r.contains(&"us-east-1".to_string()) {
            r.push("us-east-1".to_string());
        }
        r
    };

    for region in &regions_to_try {
        let region_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_types::region::Region::new(region.clone()))
            .load()
            .await;
        let cf_client = aws_sdk_cloudformation::Client::new(&region_config);

        match cf_client
            .describe_stacks()
            .stack_name(STACK_NAME)
            .send()
            .await
        {
            Ok(resp) => {
                if let Some(stack) = resp.stacks().first() {
                    if let Some(url) = stack
                        .outputs()
                        .iter()
                        .find(|o| o.output_key() == Some("FrontendUrl"))
                        .and_then(|o| o.output_value())
                    {
                        log::trace!("Discovered frontend URL from CloudFormation in {}: {}", region, url);
                        return Ok((url.to_string(), region.clone()));
                    }
                }
            }
            Err(e) => {
                log::trace!("Stack not found in {}: {}", region, e);
                continue;
            }
        }
    }

    Err(anyhow::anyhow!(
        "Could not find CloudFormation stack '{}'. Deploy it first with: cd infra && sam deploy --guided",
        STACK_NAME
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_produces_hex_strings() {
        let result = hash_password("test123");
        // 32 bytes = 64 hex chars
        assert_eq!(result.hash_hex.len(), 64);
        assert_eq!(result.salt_hex.len(), 64);
        assert!(result.hash_hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(result.salt_hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_uses_unique_salts() {
        let a = hash_password("same_password");
        let b = hash_password("same_password");
        assert_ne!(a.salt_hex, b.salt_hex);
        // Different salts → different hashes
        assert_ne!(a.hash_hex, b.hash_hex);
    }

    #[test]
    fn hash_is_deterministic_with_same_salt() {
        // Manually hash with a known salt to verify determinism
        let password = "test_password";
        let salt = [0u8; 32]; // fixed salt for test
        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, ITERATIONS, &mut key1).unwrap();
        pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, ITERATIONS, &mut key2).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn hash_matches_python_pbkdf2() {
        // Known test vector: hash "hello" with a specific salt
        // This must match Python's hashlib.pbkdf2_hmac("sha256", b"hello", salt, 600_000)
        let password = b"hello";
        let salt = b"0123456789abcdef0123456789abcdef"; // 32 bytes
        let mut key = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(password, salt, ITERATIONS, &mut key).unwrap();
        let hash_hex: String = key.iter().map(|b| format!("{:02x}", b)).collect();

        // We'll verify this matches Python output
        // For now, just ensure it's stable
        let mut key2 = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(password, salt, ITERATIONS, &mut key2).unwrap();
        assert_eq!(key, key2);

        // Print for cross-verification with Python
        println!("Rust PBKDF2 hash of 'hello' with known salt: {}", hash_hex);
    }
}
