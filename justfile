stack_name := "shuk-password-frontend"
region := "us-east-1"

# Deploy the password-protected sharing infra
deploy bucket_name:
    cd infra && sam build
    cd infra && sam deploy \
        --stack-name {{stack_name}} \
        --resolve-s3 \
        --capabilities CAPABILITY_IAM \
        --region {{region}} \
        --no-confirm-changeset \
        --parameter-overrides BucketName={{bucket_name}}

# Tear down the infra
destroy:
    sam delete --stack-name {{stack_name}} --region {{region}} --no-prompts
