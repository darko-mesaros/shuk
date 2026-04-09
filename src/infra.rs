use aws_sdk_cloudformation::types::{Capability, Parameter, StackStatus};
use aws_sdk_cloudformation::Client as CfnClient;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::time::Duration;

use crate::utils;

const STACK_NAME: &str = "shuk-password-frontend";
const LAMBDA_S3_KEY: &str = "shuk-infra/lambda.zip";
const INFRA_STATE_FILE: &str = "infra.toml";

// --- Embedded Lambda code ---
const LAMBDA_CODE: &str = include_str!("../infra/lambda/app.py");

// --- Embedded CloudFormation template ---
// Placeholders: {{BUCKET_NAME}}, {{LAMBDA_S3_BUCKET}}, {{LAMBDA_S3_KEY}}
const CFN_TEMPLATE: &str = r#"AWSTemplateFormatVersion: '2010-09-09'
Description: Shuk password-protected file sharing frontend

Parameters:
  BucketName:
    Type: String
  LambdaS3Bucket:
    Type: String
  LambdaS3Key:
    Type: String

Resources:
  SharesTable:
    Type: AWS::DynamoDB::Table
    Properties:
      TableName: shuk-password-shares
      BillingMode: PAY_PER_REQUEST
      AttributeDefinitions:
        - AttributeName: share_id
          AttributeType: S
      KeySchema:
        - AttributeName: share_id
          KeyType: HASH
      TimeToLiveSpecification:
        AttributeName: expiry_ttl
        Enabled: true

  LambdaRole:
    Type: AWS::IAM::Role
    Properties:
      AssumeRolePolicyDocument:
        Version: '2012-10-17'
        Statement:
          - Effect: Allow
            Principal:
              Service: lambda.amazonaws.com
            Action: sts:AssumeRole
      ManagedPolicyArns:
        - arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
      Policies:
        - PolicyName: ShukSharePolicy
          PolicyDocument:
            Version: '2012-10-17'
            Statement:
              - Effect: Allow
                Action:
                  - dynamodb:GetItem
                  - dynamodb:PutItem
                  - dynamodb:UpdateItem
                Resource: !GetAtt SharesTable.Arn
              - Effect: Allow
                Action:
                  - s3:GetObject
                Resource: !Sub 'arn:aws:s3:::${BucketName}/*'

  ShareFunction:
    Type: AWS::Lambda::Function
    Properties:
      FunctionName: shuk-password-share
      Runtime: python3.12
      Handler: app.handler
      Architectures:
        - arm64
      MemorySize: 128
      Timeout: 10
      Role: !GetAtt LambdaRole.Arn
      Environment:
        Variables:
          TABLE_NAME: !Ref SharesTable
      Code:
        S3Bucket: !Ref LambdaS3Bucket
        S3Key: !Ref LambdaS3Key

  ShareApi:
    Type: AWS::ApiGatewayV2::Api
    Properties:
      Name: shuk-password-api
      ProtocolType: HTTP

  ShareApiStage:
    Type: AWS::ApiGatewayV2::Stage
    Properties:
      ApiId: !Ref ShareApi
      StageName: $default
      AutoDeploy: true
      DefaultRouteSettings:
        ThrottlingBurstLimit: 10
        ThrottlingRateLimit: 5

  LambdaIntegration:
    Type: AWS::ApiGatewayV2::Integration
    Properties:
      ApiId: !Ref ShareApi
      IntegrationType: AWS_PROXY
      IntegrationUri: !GetAtt ShareFunction.Arn
      PayloadFormatVersion: '2.0'

  GetRoute:
    Type: AWS::ApiGatewayV2::Route
    Properties:
      ApiId: !Ref ShareApi
      RouteKey: GET /share/{share_id}
      Target: !Sub 'integrations/${LambdaIntegration}'

  PostRoute:
    Type: AWS::ApiGatewayV2::Route
    Properties:
      ApiId: !Ref ShareApi
      RouteKey: POST /share/{share_id}
      Target: !Sub 'integrations/${LambdaIntegration}'

  LambdaPermission:
    Type: AWS::Lambda::Permission
    Properties:
      FunctionName: !Ref ShareFunction
      Action: lambda:InvokeFunction
      Principal: apigateway.amazonaws.com
      SourceArn: !Sub 'arn:aws:execute-api:${AWS::Region}:${AWS::AccountId}:${ShareApi}/*'

Outputs:
  FrontendUrl:
    Description: URL for the password-protected share frontend
    Value: !Sub 'https://${ShareApi}.execute-api.${AWS::Region}.amazonaws.com'
"#;

// --- Local state tracking ---
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct InfraState {
    pub stack_name: String,
    pub region: String,
    pub bucket_name: String,
    pub frontend_url: String,
    pub deployed_at: String,
    pub custom_domain: Option<String>,
    pub certificate_arn: Option<String>,
    pub hosted_zone_id: Option<String>,
}

impl InfraState {
    pub fn load() -> Option<Self> {
        let path = dirs::home_dir()?.join(".config/shuk").join(INFRA_STATE_FILE);
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        let dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join(".config/shuk");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(INFRA_STATE_FILE);
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn delete() -> Result<(), anyhow::Error> {
        let path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join(".config/shuk")
            .join(INFRA_STATE_FILE);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

// --- Deploy ---
pub async fn deploy_infra() -> Result<(), anyhow::Error> {
    println!("{}", "🔧 Shuk Infrastructure Setup".yellow().bold());
    println!("========================================\n");

    // Step 1: Check AWS credentials
    println!("{}", "Step 1: Checking AWS credentials...".bold());
    let shuk_config = utils::Config::load_config()?;
    let region = prompt_with_default(
        "Deploy region",
        shuk_config.fallback_region.as_deref().unwrap_or("us-east-1"),
    )?;
    let aws_config = utils::configure_aws(region.clone(), shuk_config.aws_profile.as_ref()).await;

    let sts = aws_sdk_sts::Client::new(&aws_config);
    let identity = sts
        .get_caller_identity()
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("AWS authentication failed. Check your credentials."))?;
    println!(
        "  {} Authenticated as {}",
        "✅".green(),
        identity.arn().unwrap_or("unknown")
    );
    println!("  📍 Region: {}\n", &region);

    // Step 2: Check existing deployment
    println!("{}", "Step 2: Checking existing deployment...".bold());
    let cfn = CfnClient::new(&aws_config);
    let existing = get_stack_status(&cfn).await;
    match &existing {
        Some(status) => {
            println!("  ℹ️  Existing stack found: {:?}", status);
            if !confirm("  Update the existing deployment?")? {
                println!("Aborted.");
                return Ok(());
            }
        }
        None => println!("  ℹ️  No existing stack found. Will create new.\n"),
    }

    // Step 3: Configuration
    println!("{}", "Step 3: Configuration".bold());
    let bucket = &shuk_config.bucket_name;
    println!("  S3 bucket (from shuk.toml): {}", bucket);
    println!();

    // Cost estimate
    println!("{}", "💰 Estimated cost:".bold());
    println!("  This deploys serverless infrastructure — you only pay for what you use.\n");
    println!("  {}  On-demand: $1.25/M writes, $0.25/M reads. 25 GB storage always free.", "DynamoDB:".cyan());
    println!("  {}    1M requests + 400K GB-seconds/month always free.", "Lambda:".cyan());
    println!("  {} $1.00/M requests. First 1M/month free for 12 months.", "API Gateway:".cyan());
    println!();
    println!("  For typical usage (< 1000 shares/month): {}", "effectively $0.00".green().bold());
    println!("  More info: {}", "https://aws.amazon.com/free/".underline());
    println!();

    if !confirm("  Proceed with deployment?")? {
        println!("Aborted.");
        return Ok(());
    }
    println!();

    // Step 4: Package and upload Lambda
    println!("{}", "Step 4: Packaging Lambda...".bold());
    let s3 = S3Client::new(&aws_config);
    zip_and_upload_lambda(&s3, bucket).await?;
    println!("  {} Uploaded to s3://{}/{}\n", "✅".green(), bucket, LAMBDA_S3_KEY);

    // Step 5: Deploy CloudFormation
    println!("{}", "Step 5: Deploying CloudFormation stack...".bold());
    let params = vec![
        make_param("BucketName", bucket),
        make_param("LambdaS3Bucket", bucket),
        make_param("LambdaS3Key", LAMBDA_S3_KEY),
    ];

    if existing.is_some() {
        cfn.update_stack()
            .stack_name(STACK_NAME)
            .template_body(CFN_TEMPLATE)
            .set_parameters(Some(params))
            .capabilities(Capability::CapabilityIam)
            .send()
            .await?;
    } else {
        cfn.create_stack()
            .stack_name(STACK_NAME)
            .template_body(CFN_TEMPLATE)
            .set_parameters(Some(params))
            .capabilities(Capability::CapabilityIam)
            .send()
            .await?;
    }

    print!("  ⏳ Waiting for stack...");
    io::stdout().flush()?;
    let frontend_url = wait_for_stack(&cfn).await?;
    println!("\n  {} Stack ready!\n", "✅".green());

    // Optional: Custom domain
    let mut custom_domain: Option<String> = None;
    let mut certificate_arn: Option<String> = None;
    let mut hosted_zone_id: Option<String> = None;
    let final_url;

    println!("{}", "Step 6: Custom domain (optional)".bold());
    println!("  You can use a subdomain of a domain you already own in Route53.");
    println!("  Example: if you own example.com, use {} for share links.", "share.example.com".cyan());
    println!("  Your existing domain/website will {} be affected.\n", "not".bold());

    if confirm("  Set up a custom domain?")? {
        let domain = prompt("  Enter subdomain (e.g., share.yourdomain.com)")?;
        if domain.is_empty() {
            println!("  Skipping custom domain.\n");
            final_url = frontend_url.clone();
        } else {
            match setup_custom_domain(&aws_config, &domain, &frontend_url, &region).await {
                Ok(domain_info) => {
                    custom_domain = Some(domain_info.domain);
                    certificate_arn = Some(domain_info.certificate_arn);
                    hosted_zone_id = Some(domain_info.hosted_zone_id);
                    final_url = format!("https://{}", custom_domain.as_ref().unwrap());
                }
                Err(e) => {
                    println!("  {} Custom domain setup failed: {}", "⚠️".yellow(), e);
                    println!("  Continuing with the default API Gateway URL.\n");
                    final_url = frontend_url.clone();
                }
            }
        }
    } else {
        println!();
        final_url = frontend_url.clone();
    }

    // Step 7: Save state
    println!("{}", "Step 7: Saving configuration...".bold());
    let state = InfraState {
        stack_name: STACK_NAME.into(),
        region: region.clone(),
        bucket_name: bucket.clone(),
        frontend_url: final_url.clone(),
        deployed_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        custom_domain,
        certificate_arn,
        hosted_zone_id,
    };
    state.save()?;
    println!("  {} State saved to ~/.config/shuk/{}", "✅".green(), INFRA_STATE_FILE);

    // Update shuk.toml with frontend URL
    update_config_frontend_url(&final_url)?;
    println!("  {} Updated shuk.toml with frontend URL", "✅".green());

    println!("\n========================================");
    println!("🔗 Frontend URL: {}", final_url.green().bold());
    println!(
        "🎉 Done! You can now use: {}",
        "shuk file.zip --password mysecret".cyan()
    );

    Ok(())
}

// --- Destroy ---
pub async fn destroy_infra() -> Result<(), anyhow::Error> {
    println!("{}", "🗑️  Shuk Infrastructure Teardown".yellow().bold());
    println!("========================================\n");

    let state = InfraState::load()
        .ok_or_else(|| anyhow::anyhow!("No deployment found. Nothing to destroy."))?;

    println!("  Stack:  {}", state.stack_name);
    println!("  Region: {}", state.region);
    println!("  URL:    {}", state.frontend_url);
    if let Some(ref domain) = state.custom_domain {
        println!("  Domain: {}", domain);
    }
    println!();

    if !confirm("Are you sure you want to destroy this?")? {
        println!("Aborted.");
        return Ok(());
    }

    let aws_config = utils::configure_aws(state.region.clone(), None).await;
    let cfn = CfnClient::new(&aws_config);

    // Clean up custom domain resources first (before stack deletion)
    if let Some(ref domain) = state.custom_domain {
        println!("\n  🗑️  Removing custom domain...");
        let apigw = aws_sdk_apigatewayv2::Client::new(&aws_config);
        let _ = apigw.delete_domain_name().domain_name(domain).send().await;

        if let Some(ref zone_id) = state.hosted_zone_id {
            let r53 = aws_sdk_route53::Client::new(&aws_config);
            // Delete the A record
            let _ = delete_alias_record(&r53, zone_id, domain, &state.region).await;
        }

        if let Some(ref cert_arn) = state.certificate_arn {
            let acm = aws_sdk_acm::Client::new(&aws_config);
            let _ = acm.delete_certificate().certificate_arn(cert_arn).send().await;
        }
        println!("  {} Custom domain resources cleaned up.", "✅".green());
    }

    println!("\n  🗑️  Deleting stack...");
    cfn.delete_stack()
        .stack_name(&state.stack_name)
        .send()
        .await?;

    print!("  ⏳ Waiting...");
    io::stdout().flush()?;
    wait_for_delete(&cfn).await?;
    println!("\n  {} Stack deleted.", "✅".green());

    // Clean up Lambda zip from S3
    let s3 = S3Client::new(&aws_config);
    let _ = s3
        .delete_object()
        .bucket(&state.bucket_name)
        .key(LAMBDA_S3_KEY)
        .send()
        .await;
    println!("  {} Cleaned up Lambda package from S3.", "✅".green());

    // Remove local state
    InfraState::delete()?;
    remove_config_frontend_url()?;
    println!("  {} Removed local state and config.", "✅".green());

    println!("\n========================================");
    println!("🎉 Infrastructure destroyed.");

    Ok(())
}

// --- Status ---
pub async fn infra_status() -> Result<(), anyhow::Error> {
    println!("{}", "📊 Shuk Infrastructure Status".yellow().bold());
    println!("========================================\n");

    let state = match InfraState::load() {
        Some(s) => s,
        None => {
            println!("  No deployment found.");
            println!(
                "  Run {} to set up the password-sharing backend.",
                "--deploy-infra".cyan()
            );
            return Ok(());
        }
    };

    println!("  {}", "Local state:".bold());
    println!("    Stack:       {}", state.stack_name);
    println!("    Region:      {}", state.region);
    println!("    Bucket:      {}", state.bucket_name);
    println!("    Frontend:    {}", state.frontend_url);
    if let Some(ref domain) = state.custom_domain {
        println!("    Domain:      {}", domain);
    }
    println!("    Deployed at: {}", state.deployed_at);

    // Query live status
    let aws_config = utils::configure_aws(state.region.clone(), None).await;
    let cfn = CfnClient::new(&aws_config);

    println!("\n  {}", "Live stack status:".bold());
    match get_stack_info(&cfn).await {
        Some((status, url)) => {
            let status_str = format!("{:?}", status);
            let colored = if status_str.contains("COMPLETE") && !status_str.contains("DELETE") {
                status_str.green().to_string()
            } else if status_str.contains("PROGRESS") {
                status_str.yellow().to_string()
            } else {
                status_str.red().to_string()
            };
            println!("    Status:   {}", colored);
            if let Some(u) = url {
                println!("    URL:      {}", u);
            }
        }
        None => {
            println!("    {} Stack not found in AWS.", "⚠️".yellow());
            println!("    Local state may be stale. Run --deploy-infra to redeploy.");
        }
    }

    Ok(())
}

// --- Custom domain ---

struct DomainInfo {
    domain: String,
    certificate_arn: String,
    hosted_zone_id: String,
}

async fn setup_custom_domain(
    aws_config: &aws_config::SdkConfig,
    domain: &str,
    api_url: &str,
    region: &str,
) -> Result<DomainInfo, anyhow::Error> {
    let r53 = aws_sdk_route53::Client::new(aws_config);
    let acm = aws_sdk_acm::Client::new(aws_config);
    let apigw = aws_sdk_apigatewayv2::Client::new(aws_config);

    // Step 1: Find the hosted zone
    println!("\n  🔍 Looking up Route53 hosted zone...");
    let (zone_id, zone_name) = find_hosted_zone(&r53, domain).await?;
    println!("  {} Found hosted zone: {} ({})", "✅".green(), zone_name, zone_id);

    // Step 2: Check for existing DNS record
    println!("  🔍 Checking for existing DNS records for {}...", domain.cyan());
    let existing = check_existing_record(&r53, &zone_id, domain).await?;
    if let Some(record_type) = existing {
        println!();
        println!("  {}", "⚠️  WARNING: An existing DNS record was found!".yellow().bold());
        println!("    Domain: {}", domain.cyan());
        println!("    Type:   {}", record_type.yellow());
        println!("    This will {} the existing record.", "overwrite".red().bold());
        println!();
        if !confirm("  Are you SURE you want to overwrite this record?")? {
            return Err(anyhow::anyhow!("User cancelled — existing record preserved."));
        }
    } else {
        println!("  {} No existing record — safe to create.", "✅".green());
    }

    // Step 3: Request ACM certificate
    println!("  📜 Requesting ACM certificate for {}...", domain.cyan());
    let cert_arn = request_certificate(&acm, domain).await?;
    println!("  {} Certificate requested: {}", "✅".green(), &cert_arn[..60.min(cert_arn.len())]);

    // Step 4: DNS validation — add the CNAME record to Route53
    println!("  ⏳ Waiting for DNS validation details...");
    let (validation_name, validation_value) = wait_for_validation_details(&acm, &cert_arn).await?;
    println!("  📝 Adding DNS validation record to Route53...");
    upsert_cname_record(&r53, &zone_id, &validation_name, &validation_value).await?;

    // Step 5: Wait for certificate to be issued
    print!("  ⏳ Waiting for certificate validation (this may take a few minutes)...");
    io::stdout().flush()?;
    wait_for_certificate(&acm, &cert_arn).await?;
    println!("\n  {} Certificate issued!", "✅".green());

    // Step 6: Create API Gateway custom domain
    println!("  🔗 Creating API Gateway custom domain mapping...");
    let apigw_domain_name = create_api_domain(&apigw, domain, &cert_arn, api_url, region).await?;

    // Step 7: Create Route53 alias record
    println!("  📝 Creating Route53 alias record...");
    create_alias_record(&r53, &zone_id, domain, &apigw_domain_name, region).await?;
    println!("  {} Custom domain configured: {}", "✅".green(), format!("https://{}", domain).green().bold());
    println!();

    Ok(DomainInfo {
        domain: domain.to_string(),
        certificate_arn: cert_arn,
        hosted_zone_id: zone_id,
    })
}

/// Extract the parent domain and find its hosted zone
async fn find_hosted_zone(
    r53: &aws_sdk_route53::Client,
    domain: &str,
) -> Result<(String, String), anyhow::Error> {
    let resp = r53.list_hosted_zones().send().await?;
    // Try matching from most specific to least specific
    // e.g., for "share.sub.example.com" try "sub.example.com." then "example.com."
    let parts: Vec<&str> = domain.split('.').collect();
    for i in 1..parts.len() {
        let candidate = format!("{}.", parts[i..].join("."));
        if let Some(zone) = resp.hosted_zones().iter().find(|z| z.name() == candidate) {
            let id = zone.id().trim_start_matches("/hostedzone/").to_string();
            return Ok((id, candidate.trim_end_matches('.').to_string()));
        }
    }
    Err(anyhow::anyhow!(
        "No Route53 hosted zone found for '{}'. Make sure the parent domain is managed in Route53.",
        domain
    ))
}

async fn check_existing_record(
    r53: &aws_sdk_route53::Client,
    zone_id: &str,
    domain: &str,
) -> Result<Option<String>, anyhow::Error> {
    let fqdn = format!("{}.", domain);
    let resp = r53
        .list_resource_record_sets()
        .hosted_zone_id(zone_id)
        .start_record_name(&fqdn)
        .max_items(10)
        .send()
        .await?;

    for rrs in resp.resource_record_sets() {
        if rrs.name() == fqdn {
            return Ok(Some(rrs.r#type().as_str().to_string()));
        }
    }
    Ok(None)
}

async fn request_certificate(
    acm: &aws_sdk_acm::Client,
    domain: &str,
) -> Result<String, anyhow::Error> {
    let resp = acm
        .request_certificate()
        .domain_name(domain)
        .validation_method(aws_sdk_acm::types::ValidationMethod::Dns)
        .send()
        .await?;
    resp.certificate_arn()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("ACM did not return a certificate ARN"))
}

async fn wait_for_validation_details(
    acm: &aws_sdk_acm::Client,
    cert_arn: &str,
) -> Result<(String, String), anyhow::Error> {
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let resp = acm
            .describe_certificate()
            .certificate_arn(cert_arn)
            .send()
            .await?;
        if let Some(cert) = resp.certificate() {
            if let Some(dvo) = cert.domain_validation_options().first() {
                if let Some(rr) = dvo.resource_record() {
                    return Ok((rr.name().to_string(), rr.value().to_string()));
                }
            }
        }
    }
    Err(anyhow::anyhow!("Timed out waiting for ACM validation details"))
}

async fn upsert_cname_record(
    r53: &aws_sdk_route53::Client,
    zone_id: &str,
    name: &str,
    value: &str,
) -> Result<(), anyhow::Error> {
    use aws_sdk_route53::types::{
        Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType,
    };
    r53.change_resource_record_sets()
        .hosted_zone_id(zone_id)
        .change_batch(
            ChangeBatch::builder()
                .changes(
                    Change::builder()
                        .action(ChangeAction::Upsert)
                        .resource_record_set(
                            ResourceRecordSet::builder()
                                .name(name)
                                .r#type(RrType::Cname)
                                .ttl(300)
                                .resource_records(
                                    ResourceRecord::builder().value(value).build()?,
                                )
                                .build()?,
                        )
                        .build()?,
                )
                .build()?,
        )
        .send()
        .await?;
    Ok(())
}

async fn wait_for_certificate(
    acm: &aws_sdk_acm::Client,
    cert_arn: &str,
) -> Result<(), anyhow::Error> {
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        print!(".");
        io::stdout().flush()?;
        let resp = acm
            .describe_certificate()
            .certificate_arn(cert_arn)
            .send()
            .await?;
        if let Some(cert) = resp.certificate() {
            match cert.status() {
                Some(aws_sdk_acm::types::CertificateStatus::Issued) => return Ok(()),
                Some(aws_sdk_acm::types::CertificateStatus::Failed) => {
                    return Err(anyhow::anyhow!("Certificate validation failed"));
                }
                _ => continue,
            }
        }
    }
    Err(anyhow::anyhow!("Timed out waiting for certificate validation (5 minutes)"))
}

async fn create_api_domain(
    apigw: &aws_sdk_apigatewayv2::Client,
    domain: &str,
    cert_arn: &str,
    api_url: &str,
    _region: &str,
) -> Result<String, anyhow::Error> {
    use aws_sdk_apigatewayv2::types::DomainNameConfiguration;

    // Extract API ID from URL: https://{api_id}.execute-api.{region}.amazonaws.com
    let api_id = api_url
        .trim_start_matches("https://")
        .split('.')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Could not parse API ID from URL"))?;

    let domain_config = DomainNameConfiguration::builder()
        .certificate_arn(cert_arn)
        .endpoint_type(aws_sdk_apigatewayv2::types::EndpointType::Regional)
        .security_policy(aws_sdk_apigatewayv2::types::SecurityPolicy::Tls12)
        .build();

    let resp = apigw
        .create_domain_name()
        .domain_name(domain)
        .domain_name_configurations(domain_config)
        .send()
        .await?;

    // Get the target domain name for the alias record
    let target = resp
        .domain_name_configurations()
        .first()
        .and_then(|c| c.api_gateway_domain_name())
        .ok_or_else(|| anyhow::anyhow!("API Gateway did not return a domain target"))?
        .to_string();

    // Create API mapping
    apigw
        .create_api_mapping()
        .domain_name(domain)
        .api_id(api_id)
        .stage("$default")
        .send()
        .await?;

    Ok(target)
}

async fn create_alias_record(
    r53: &aws_sdk_route53::Client,
    zone_id: &str,
    domain: &str,
    target_domain: &str,
    region: &str,
) -> Result<(), anyhow::Error> {
    use aws_sdk_route53::types::{
        AliasTarget, Change, ChangeAction, ChangeBatch, ResourceRecordSet, RrType,
    };

    // API Gateway hosted zone IDs per region
    // https://docs.aws.amazon.com/general/latest/gr/apigateway.html
    let apigw_zone_id = get_apigw_hosted_zone_id(region);

    r53.change_resource_record_sets()
        .hosted_zone_id(zone_id)
        .change_batch(
            ChangeBatch::builder()
                .changes(
                    Change::builder()
                        .action(ChangeAction::Upsert)
                        .resource_record_set(
                            ResourceRecordSet::builder()
                                .name(domain)
                                .r#type(RrType::A)
                                .alias_target(
                                    AliasTarget::builder()
                                        .dns_name(target_domain)
                                        .hosted_zone_id(apigw_zone_id)
                                        .evaluate_target_health(false)
                                        .build()?,
                                )
                                .build()?,
                        )
                        .build()?,
                )
                .build()?,
        )
        .send()
        .await?;
    Ok(())
}

async fn delete_alias_record(
    r53: &aws_sdk_route53::Client,
    zone_id: &str,
    domain: &str,
    _region: &str,
) -> Result<(), anyhow::Error> {
    use aws_sdk_route53::types::{
        AliasTarget, Change, ChangeAction, ChangeBatch, ResourceRecordSet, RrType,
    };

    // We need to find the current alias target to delete it
    let fqdn = format!("{}.", domain);
    let resp = r53
        .list_resource_record_sets()
        .hosted_zone_id(zone_id)
        .start_record_name(&fqdn)
        .max_items(5)
        .send()
        .await?;

    for rrs in resp.resource_record_sets() {
        if rrs.name() == fqdn && rrs.r#type() == &RrType::A {
            if let Some(alias) = rrs.alias_target() {
                r53.change_resource_record_sets()
                    .hosted_zone_id(zone_id)
                    .change_batch(
                        ChangeBatch::builder()
                            .changes(
                                Change::builder()
                                    .action(ChangeAction::Delete)
                                    .resource_record_set(
                                        ResourceRecordSet::builder()
                                            .name(domain)
                                            .r#type(RrType::A)
                                            .alias_target(
                                                AliasTarget::builder()
                                                    .dns_name(alias.dns_name())
                                                    .hosted_zone_id(alias.hosted_zone_id())
                                                    .evaluate_target_health(false)
                                                    .build()?,
                                            )
                                            .build()?,
                                    )
                                    .build()?,
                            )
                            .build()?,
                    )
                    .send()
                    .await?;
                break;
            }
        }
    }
    Ok(())
}

/// Returns the API Gateway regional hosted zone ID for a given AWS region
fn get_apigw_hosted_zone_id(region: &str) -> &'static str {
    match region {
        "us-east-1" => "Z1UJRXOUMOOFQ8",
        "us-east-2" => "ZOJJZC49E0EPZ",
        "us-west-1" => "Z2MUQ32089INYE",
        "us-west-2" => "Z2OJLYMUO9EFXC",
        "eu-central-1" => "Z1U9ULNL0V5AJ3",
        "eu-west-1" => "ZLY8HYME6SFDD",
        "eu-west-2" => "ZJ5UAJN8Y3Z2Q",
        "eu-west-3" => "Z3KY65QIEKYHQQ",
        "eu-north-1" => "Z3UWIKFBOOGXPP",
        "ap-northeast-1" => "Z1YSHQZHG15GKL",
        "ap-northeast-2" => "Z20JF4UZKIW1U8",
        "ap-southeast-1" => "ZL327KTPIQFUL",
        "ap-southeast-2" => "Z2RPCDW04V8134",
        "ap-south-1" => "Z3VO1T2WNX0TKB",
        "sa-east-1" => "ZCMLWB8V5SYIT",
        "ca-central-1" => "Z19DQILCV0OWEC",
        _ => "Z1UJRXOUMOOFQ8", // default to us-east-1
    }
}

// --- Helpers ---

async fn zip_and_upload_lambda(s3: &S3Client, bucket: &str) -> Result<(), anyhow::Error> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("app.py", options)?;
        zip.write_all(LAMBDA_CODE.as_bytes())?;
        zip.finish()?;
    }

    s3.put_object()
        .bucket(bucket)
        .key(LAMBDA_S3_KEY)
        .body(ByteStream::from(buf))
        .send()
        .await?;

    Ok(())
}

fn make_param(key: &str, value: &str) -> Parameter {
    Parameter::builder()
        .parameter_key(key)
        .parameter_value(value)
        .build()
}

async fn get_stack_status(cfn: &CfnClient) -> Option<StackStatus> {
    let resp = cfn
        .describe_stacks()
        .stack_name(STACK_NAME)
        .send()
        .await
        .ok()?;
    resp.stacks()
        .first()
        .and_then(|s| s.stack_status())
        .cloned()
}

async fn get_stack_info(cfn: &CfnClient) -> Option<(StackStatus, Option<String>)> {
    let resp = cfn
        .describe_stacks()
        .stack_name(STACK_NAME)
        .send()
        .await
        .ok()?;
    let stack = resp.stacks().first()?;
    let status = stack.stack_status()?.clone();
    let url = stack
        .outputs()
        .iter()
        .find(|o| o.output_key() == Some("FrontendUrl"))
        .and_then(|o| o.output_value())
        .map(|s| s.to_string());
    Some((status, url))
}

async fn wait_for_stack(cfn: &CfnClient) -> Result<String, anyhow::Error> {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        print!(".");
        io::stdout().flush()?;

        if let Some((status, url)) = get_stack_info(cfn).await {
            match status {
                StackStatus::CreateComplete | StackStatus::UpdateComplete => {
                    return url.ok_or_else(|| anyhow::anyhow!("Stack has no FrontendUrl output"));
                }
                StackStatus::CreateFailed
                | StackStatus::RollbackComplete
                | StackStatus::RollbackFailed
                | StackStatus::UpdateRollbackComplete
                | StackStatus::UpdateRollbackFailed => {
                    return Err(anyhow::anyhow!("Stack deployment failed: {:?}", status));
                }
                _ => continue, // still in progress
            }
        }
    }
}

async fn wait_for_delete(cfn: &CfnClient) -> Result<(), anyhow::Error> {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        print!(".");
        io::stdout().flush()?;

        match get_stack_status(cfn).await {
            None => return Ok(()), // Stack gone
            Some(StackStatus::DeleteComplete) => return Ok(()),
            Some(StackStatus::DeleteFailed) => {
                return Err(anyhow::anyhow!("Stack deletion failed"));
            }
            Some(_) => continue,
        }
    }
}

fn prompt_with_default(label: &str, default: &str) -> Result<String, anyhow::Error> {
    print!("  {} [{}]: ", label, default);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    Ok(if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    })
}

fn prompt(label: &str) -> Result<String, anyhow::Error> {
    print!("  {}: ", label);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn confirm(msg: &str) -> Result<bool, anyhow::Error> {
    print!("{} [y/N]: ", msg);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

fn update_config_frontend_url(url: &str) -> Result<(), anyhow::Error> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
    let path = home.join(".config/shuk").join(crate::constants::CONFIG_FILE_NAME);
    let content = std::fs::read_to_string(&path)?;

    if content.contains("password_frontend_url") {
        // Replace existing (commented or not)
        let mut new_lines: Vec<String> = Vec::new();
        for line in content.lines() {
            if line.trim_start().trim_start_matches('#').trim().starts_with("password_frontend_url") {
                new_lines.push(format!("password_frontend_url = \"{}\"", url));
            } else {
                new_lines.push(line.to_string());
            }
        }
        std::fs::write(&path, new_lines.join("\n") + "\n")?;
    } else {
        // Append
        let mut f = std::fs::OpenOptions::new().append(true).open(&path)?;
        writeln!(f, "password_frontend_url = \"{}\"", url)?;
    }
    Ok(())
}

fn remove_config_frontend_url() -> Result<(), anyhow::Error> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
    let path = home.join(".config/shuk").join(crate::constants::CONFIG_FILE_NAME);
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)?;
    let new_lines: Vec<&str> = content
        .lines()
        .filter(|l| {
            !l.trim_start()
                .trim_start_matches('#')
                .trim()
                .starts_with("password_frontend_url")
        })
        .collect();
    std::fs::write(&path, new_lines.join("\n") + "\n")?;
    Ok(())
}
