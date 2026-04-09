# Shuk Password-Protected Sharing — Implementation Plan

**Status:** ✅ Implemented and tested end-to-end on AWS (2026-04-09)
**Stack deployed in:** us-east-1, stack name `shuk-password-frontend`

## Requirements

- `shuk file.zip --password mysecretpass` — password provided inline as a flag
- Generates a link to a serverless frontend instead of a direct S3 presigned URL
- Frontend prompts for password; actual S3 download URL only revealed after correct entry
- 5 failed password attempts → link permanently invalidated
- Time-based expiration matching existing `presigned_time` config value
- Frontend URL resolution: `shuk.toml` config takes precedence, falls back to CloudFormation stack output auto-discovery
- Fixed stack name: `shuk-password-frontend`
- Low budget: serverless, pay-per-use only
- Self-service deployment: `shuk --deploy-infra` guided wizard — no SAM CLI, no manual CloudFormation
- Infrastructure fully embedded in the shuk binary (Lambda code + CloudFormation template)
- Local state tracking in `~/.config/shuk/infra.toml`
- `shuk --destroy-infra` to tear down, `shuk --infra-status` to check live status

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  shuk CLI (Rust)                                                │
│                                                                 │
│  1. Uploads file to S3 (existing flow, any region)              │
│  2. Hashes password (PBKDF2-SHA256, 600k iterations)            │
│  3. Writes share record to DynamoDB (infra region)              │
│  4. Prints: https://{apigw}/share/{uuid}                        │
└──────────┬──────────────────────────┬───────────────────────────┘
           │                          │
           ▼                          ▼
┌──────────────────┐    ┌──────────────────────────────────────┐
│  S3 Bucket       │    │  DynamoDB: shuk-password-shares       │
│  (any region)    │    │  (PAY_PER_REQUEST)                    │
│                  │    │                                        │
│                  │    │  PK: share_id (UUID)                  │
│                  │    │  password_hash, salt, bucket_name,    │
│                  │    │  object_key, region, presigned_time,  │
│                  │    │  attempts, max_attempts, expiry_ttl   │
│                  │    │  TTL enabled on expiry_ttl            │
└──────────────────┘    └──────────────────────────────────────┘
                                      ▲
                                      │
┌─────────────────────────────────────┼───────────────────────┐
│  API Gateway HTTP API               │                       │
│  (rate limited: 5/s, burst 10)      │                       │
│                                     │                       │
│  GET  /share/{share_id} ───────►┌───┴────────────────────┐  │
│  POST /share/{share_id} ───────►│  Lambda (Python 3.12)  │  │
│                                 │  arm64, 128MB           │  │
│                                 │                         │  │
│                                 │  GET:  lookup DDB       │  │
│                                 │        → serve HTML form│  │
│                                 │                         │  │
│                                 │  POST: verify hash      │  │
│                                 │        → increment fail │  │
│                                 │        → presign S3 GET │  │
│                                 │        → auto-redirect  │  │
│                                 └─────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

## Files Created/Modified

### New files

| File | Description |
|------|-------------|
| `infra/template.yaml` | SAM template — DynamoDB, Lambda, API Gateway HTTP API with throttling |
| `infra/lambda/app.py` | Python Lambda handler — GET (password form) and POST (verify + presign) |
| `infra/lambda/test_app.py` | 8 unit tests for the Lambda handler |
| `infra/README.md` | Deployment instructions |
| `src/password.rs` | Rust module — password hashing, DynamoDB share creation, frontend URL resolution |
| `src/infra.rs` | Rust module — embedded CFN template + Lambda code, deploy/destroy/status wizards |
| `justfile` | `just deploy <bucket>` and `just destroy` recipes (SAM alternative) |
| `password-plan.md` | This document |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` | Added: `aws-sdk-dynamodb`, `aws-sdk-cloudformation`, `aws-sdk-sts`, `pbkdf2`, `hmac`, `sha2`, `rand`, `uuid`, `zip` |
| `src/lib.rs` | Added `password` and `infra` modules |
| `src/main.rs` | Added `password` + `infra` module imports, wired `--password`, `--deploy-infra`, `--destroy-infra`, `--infra-status` flows, top-level error wrapper, bucket mismatch guard |
| `src/utils.rs` | Added `--password`, `--deploy-infra`, `--destroy-infra`, `--infra-status` args to `Args`, `password_frontend_url` to `Config`, `format_aws_error()` for user-friendly error messages |
| `src/constants.rs` | Added commented-out `password_frontend_url` to config template |
| `README.md` | Documented `--password` flag and password-protected sharing section |

## Technical Decisions

### Password hashing: PBKDF2-SHA256 (600k iterations, 32-byte random salt)

- Rust side: `pbkdf2` (0.12) + `hmac` + `sha2` crates
- Python side: `hashlib.pbkdf2_hmac` (stdlib, zero dependencies)
- Cross-language compatibility verified with known test vector:
  - Input: password `hello`, salt `0123456789abcdef0123456789abcdef`
  - Both produce: `f4dea4b8352a8a7f885f36abc6d817be5d068e5b0e47ad62f1b76aa0b22fc957`

### Frontend: inline HTML served by Lambda

- Single Lambda function handles both GET and POST
- No static assets, no S3 hosting, no CloudFront — minimal moving parts
- Dark theme, mobile-friendly, self-contained CSS
- Auto-redirect on successful password entry via `<script>window.location.href=...</script>`

### S3 permissions: scoped per deployment

- CloudFormation template takes a `BucketName` parameter
- Lambda IAM policy scoped to `arn:aws:s3:::{BucketName}/*`
- `--deploy-infra` reads the bucket from `shuk.toml` automatically
- Manual SAM deploy requires passing `BucketName` as a parameter override

### Rate limiting

- API Gateway HTTP API `DefaultRouteSettings`:
  - `ThrottlingRateLimit: 5` (sustained requests/second)
  - `ThrottlingBurstLimit: 10` (burst capacity)
- Returns HTTP 429 when exceeded

### Multi-region handling

- S3 bucket can be in any region (user-configured via `fallback_region`)
- Infra stack (DynamoDB + Lambda + API Gateway) can be in a different region
- The `region` field in DynamoDB records tells the Lambda which region to use for S3 presigning
- `resolve_frontend_url` tries the configured region first, then falls back to us-east-1
- DynamoDB client in the CLI is created with the infra region, not the S3 region

### Frontend URL resolution (in order)

1. `password_frontend_url` in `~/.config/shuk/shuk.toml` (if set)
2. CloudFormation `describe_stacks` on `shuk-password-frontend` → `FrontendUrl` output
3. Error with instructions to deploy the stack

### Self-service infrastructure (`src/infra.rs`)

The entire serverless backend is embedded in the shuk binary — no external tools needed.

**What's embedded:**
- Lambda Python code: `include_str!("../infra/lambda/app.py")` — compiled into the binary at build time
- CloudFormation template: raw YAML const string (no SAM transform), uses `AWS::Lambda::Function` with `S3Bucket`/`S3Key` for code

**Deploy flow (`--deploy-infra`):**
1. Verify AWS credentials via STS GetCallerIdentity
2. Prompt for deploy region (defaults to `fallback_region` from shuk.toml)
3. Read `bucket_name` from shuk.toml
4. Zip the embedded Lambda code in memory using the `zip` crate
5. Upload zip to `s3://{bucket}/shuk-infra/lambda.zip`
6. Deploy CloudFormation stack via `create_stack`/`update_stack` SDK calls
7. Poll `describe_stacks` until complete
8. Extract `FrontendUrl` from stack outputs
9. Save state to `~/.config/shuk/infra.toml`
10. Update `shuk.toml` with `password_frontend_url`

**State file (`~/.config/shuk/infra.toml`):**
```toml
stack_name = "shuk-password-frontend"
region = "us-east-1"
bucket_name = "darko-sharing"
frontend_url = "https://abc123.execute-api.us-east-1.amazonaws.com"
deployed_at = "2026-04-09T18:08:00Z"
```

**Why raw CloudFormation instead of SAM:**
- SAM requires the SAM CLI (a Python tool) — adding a Python dependency to a Rust CLI tool is a poor UX
- Raw CloudFormation can be deployed directly via the AWS SDK
- The Lambda code is too large (6KB) for CloudFormation's `ZipFile` inline limit (4096 bytes), so we zip and upload to S3 instead
- The `infra/` directory with the SAM template is kept for development and power users

### Custom domain (optional, Route53 only)

During `--deploy-infra`, the wizard offers to set up a custom subdomain:

1. User enters a subdomain (e.g., `share.shuk.rs`)
2. Wizard auto-detects the Route53 hosted zone for the parent domain (`shuk.rs`)
3. Checks for existing DNS records on that subdomain — if found, warns clearly and requires explicit confirmation before overwriting
4. Requests an ACM certificate with DNS validation
5. Adds the CNAME validation record to Route53 automatically
6. Polls until the certificate is issued (~30s to 5min)
7. Creates an API Gateway custom domain mapping
8. Creates a Route53 A record (alias) pointing to the API Gateway domain

**Key UX decisions:**
- Subdomain only — the user's existing domain/website is never touched
- Existing record check with explicit overwrite confirmation
- Route53-only — non-Route53 DNS providers are not supported
- Graceful failure — if custom domain setup fails, falls back to the default API Gateway URL
- All domain resources tracked in `infra.toml` and cleaned up on `--destroy-infra`

**Resources tracked for cleanup:**
- `custom_domain` — the subdomain name
- `certificate_arn` — ACM certificate ARN
- `hosted_zone_id` — Route53 hosted zone ID

### Error handling (`utils::format_aws_error`)

All errors flow through a top-level wrapper in `main()` that catches any `anyhow::Error` and formats it via `format_aws_error()` before printing. This ensures users never see raw SDK error dumps.

**Error mapping:**

| Raw error contains | User sees |
|---|---|
| `credentials`, `InvalidConfiguration`, `credentials-login` | `AWS credentials error. Please check your credentials...` |
| `ExpiredToken`, `expired`, `security token` | `AWS session has expired. Please re-authenticate...` |
| `dispatch failure`, `DispatchFailure` | `Could not connect to AWS. Please check your credentials and network...` |
| `NoSuchBucket` | `S3 bucket not found. Check 'bucket_name' in ~/.config/shuk/shuk.toml` |
| `NoSuchKey` | `File not found in S3. It may have been deleted.` |
| `AccessDenied`, `Forbidden` | `Access denied. Your AWS credentials don't have permission...` |
| `Could not find CloudFormation stack` | `Password-sharing infrastructure not deployed. Run 'shuk --deploy-infra'...` |
| Config file read/parse errors | `Could not load shuk configuration. Run 'shuk --init'...` |
| `ResourceNotFoundException` (DynamoDB) | `DynamoDB table not found... Run 'shuk --deploy-infra'...` |
| Anything else | Passes through as-is |

### Bucket mismatch guard

When `--password` is used, shuk checks `~/.config/shuk/infra.toml` to compare the deployed bucket against the current `bucket_name` in `shuk.toml`. If they differ, shuk exits immediately with a clear error:

```
⚠️  Bucket mismatch detected!
  Your shuk.toml uses bucket: new-bucket
  But the deployed infra is scoped to: old-bucket
  The recipient won't be able to download the file.

  Run shuk --deploy-infra to update the infrastructure.
```

This only fires on the `--password` path — normal presigned URL sharing is unaffected.

## DynamoDB Schema

| Attribute | Type | Description |
|-----------|------|-------------|
| `share_id` (PK) | S | UUID v4, used in the share URL path |
| `password_hash` | S | PBKDF2-SHA256 hex digest (64 chars) |
| `salt` | S | Random salt hex (64 chars) |
| `bucket_name` | S | S3 bucket name |
| `object_key` | S | Full S3 key including prefix |
| `presigned_time` | N | Seconds for the download presigned URL |
| `expiry_ttl` | N | Unix timestamp — DynamoDB TTL auto-deletes expired records |
| `attempts` | N | Failed password attempt counter |
| `max_attempts` | N | 5 |
| `region` | S | AWS region where the S3 bucket lives |

## Config additions

```toml
# Optional: override the auto-discovered frontend URL
# password_frontend_url = "https://your-api-id.execute-api.us-east-1.amazonaws.com"
```

## Deployment

The infrastructure is fully self-contained in the shuk binary. No SAM CLI, no cloning repos, no manual CloudFormation.

### Deploy
```bash
shuk --deploy-infra
```

This runs a guided wizard that:
1. Verifies AWS credentials (STS GetCallerIdentity)
2. Prompts for deploy region (defaults to fallback_region from shuk.toml)
3. Reads bucket_name from shuk.toml
4. Zips the embedded Lambda code in memory
5. Uploads the zip to `s3://{bucket}/shuk-infra/lambda.zip`
6. Deploys a raw CloudFormation stack (no SAM transform) via the SDK
7. Waits for completion, extracts the FrontendUrl output
8. Saves state to `~/.config/shuk/infra.toml`
9. Updates `shuk.toml` with `password_frontend_url`

### Check status
```bash
shuk --infra-status
```

Shows local state (stack name, region, URL, deploy time) and queries live CloudFormation status.

### Destroy
```bash
shuk --destroy-infra
```

Deletes the CloudFormation stack, cleans up the Lambda zip from S3, removes local state and config.

### Manual deployment (alternative)

The `infra/` directory contains a SAM template for manual deployment:
```bash
cd infra && sam build && sam deploy \
  --stack-name shuk-password-frontend \
  --resolve-s3 \
  --capabilities CAPABILITY_IAM \
  --parameter-overrides BucketName=<your-bucket>
```

### Justfile shortcuts
```bash
just deploy <bucket-name>    # SAM deploy
just destroy                 # SAM delete
```

## Test Results (2026-04-09)

### Rust unit tests: 4 passed
- `hash_produces_hex_strings` — correct format
- `hash_uses_unique_salts` — different salts per call
- `hash_is_deterministic_with_same_salt` — same inputs → same output
- `hash_matches_python_pbkdf2` — cross-language compatibility

### Python Lambda unit tests: 8 passed
- GET: valid share returns form, missing returns 404, expired returns 410, locked returns 410
- POST: correct password returns presigned URL, wrong password increments attempts, 5th wrong locks, locked rejects correct password

### End-to-end on AWS: all passed
- Upload with `--password` → file uploaded, DynamoDB record created, share URL printed
- GET share URL → password form rendered
- Wrong password → "Wrong password. N attempts remaining."
- Correct password → "Password accepted" → file downloads (`Hello from shuk password test!`)
- 5 wrong attempts → link permanently locked, even correct password rejected
- Rate limiting → HTTP 429 returned under burst load (30 parallel requests)
- CloudFormation auto-discovery → correctly found stack in us-east-1 while S3 bucket was in eu-central-1

## Known Limitations / Future Work

- No download count limit — once unlocked, unlimited downloads until expiry
- No WAF — rate limiting is API Gateway-level only
- DynamoDB encryption uses default AWS-owned key (not KMS)
- Single bucket per deployment — if user changes buckets, they need to redeploy the stack (`shuk --deploy-infra` handles updates)
- The `credentials-login` feature is not enabled in the Rust AWS SDK — users with `aws login` sessions need to export credentials as env vars or use a named profile
- Custom domain support via Route53 — only Route53-managed domains supported (no Cloudflare/Namecheap)
- `--deploy-infra` tested end-to-end: raw CloudFormation stack deployed to eu-central-1 successfully
