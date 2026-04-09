# Shuk Password-Protected Sharing — Infrastructure

This directory contains the AWS SAM template for the password-protected file sharing frontend.

## Architecture

- **API Gateway HTTP API** — serves the password form and handles verification
- **Lambda (Python 3.12, arm64)** — single function handling GET (form) and POST (verify + presign)
- **DynamoDB** — stores share records (password hash, S3 location, attempt counter, TTL)

## Prerequisites

- [AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html)
- AWS credentials configured

## Deploy

```bash
cd infra
sam build
sam deploy --guided --stack-name shuk-password-frontend
```

On first deploy, SAM will prompt for configuration — including your S3 bucket name (the same one in your `shuk.toml`). This scopes the Lambda's S3 permissions to only that bucket. The stack name **must** be `shuk-password-frontend` for auto-discovery to work.

After deployment, the API Gateway URL will be shown in the stack outputs. You can optionally add it to your `~/.config/shuk/shuk.toml`:

```toml
password_frontend_url = "https://your-api-id.execute-api.us-east-1.amazonaws.com"
```

If you don't set this, shuk will auto-discover the URL from CloudFormation.

## Update

```bash
cd infra
sam build
sam deploy
```

## Remove

```bash
sam delete --stack-name shuk-password-frontend
```

## How It Works

1. `shuk file.zip --password mysecret` uploads the file to S3, hashes the password (PBKDF2-SHA256, 600k iterations), and writes a share record to DynamoDB
2. The share URL points to the API Gateway endpoint: `https://.../share/{share_id}`
3. Recipient visits the URL → Lambda serves a password form
4. Recipient enters the password → Lambda verifies the hash, generates a presigned S3 download URL
5. After 5 wrong attempts, the link is permanently locked
6. Share records expire automatically via DynamoDB TTL (matching your `presigned_time` config)
