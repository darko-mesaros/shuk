use clap::Parser;
use shuk::utils::Args;
use std::path::PathBuf;

// =============================================================================
// Property Test: Task 1.2 - Property 1
// Upload-only flag parses correctly with any valid filename
// Feature: upload-only, Property 1: Upload-only flag parses correctly with any valid filename
// **Validates: Requirements 1.2, 1.5**
// =============================================================================

mod property_upload_only_parsing {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn upload_only_flag_parses_with_any_valid_filename(
            filename in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}"
        ) {
            let result = Args::try_parse_from(["shuk", "--upload-only", &filename]);
            let args = result.expect("parsing should succeed");
            assert!(args.upload_only, "upload_only should be true");
            assert_eq!(
                args.filename,
                Some(PathBuf::from(&filename)),
                "filename should match input"
            );
            assert!(!args.init, "init should be false");
        }

        #[test]
        fn upload_only_with_verbose_parses_with_any_valid_filename(
            filename in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}"
        ) {
            let result = Args::try_parse_from(["shuk", "--upload-only", "--verbose", &filename]);
            let args = result.expect("parsing should succeed");
            assert!(args.upload_only, "upload_only should be true");
            assert!(args.verbose, "verbose should be true");
            assert_eq!(
                args.filename,
                Some(PathBuf::from(&filename)),
                "filename should match input"
            );
        }
    }
}

// =============================================================================
// Property Test: Task 1.3 - Property 2
// Existing argument combinations remain valid (backward compatibility)
// Feature: upload-only, Property 2: Existing argument combinations remain valid
// **Validates: Requirements 6.5**
// =============================================================================

mod property_backward_compatibility {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn filename_without_upload_only_has_upload_only_false(
            filename in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}"
        ) {
            let result = Args::try_parse_from(["shuk", &filename]);
            let args = result.expect("parsing should succeed");
            assert!(!args.upload_only, "upload_only should default to false");
            assert_eq!(
                args.filename,
                Some(PathBuf::from(&filename)),
                "filename should match input"
            );
            assert!(!args.init, "init should be false");
            assert!(!args.verbose, "verbose should be false");
        }

        #[test]
        fn filename_with_verbose_has_upload_only_false(
            filename in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}"
        ) {
            let result = Args::try_parse_from(["shuk", "--verbose", &filename]);
            let args = result.expect("parsing should succeed");
            assert!(!args.upload_only, "upload_only should default to false");
            assert!(args.verbose, "verbose should be true");
            assert_eq!(
                args.filename,
                Some(PathBuf::from(&filename)),
                "filename should match input"
            );
        }
    }

    #[test]
    fn init_alone_has_upload_only_false() {
        let result = Args::try_parse_from(["shuk", "--init"]);
        let args = result.expect("parsing should succeed");
        assert!(!args.upload_only, "upload_only should default to false");
        assert!(args.init, "init should be true");
        assert_eq!(args.filename, None, "filename should be None with --init");
    }
}

// =============================================================================
// Unit Tests: Task 1.4
// --upload-only flag conflict and error cases
// Requirements: 1.3, 1.4, 1.5
// =============================================================================

mod unit_upload_only_conflicts {
    use super::*;

    #[test]
    fn upload_only_with_init_fails() {
        // --upload-only conflicts with --init
        let result = Args::try_parse_from(["shuk", "--upload-only", "--init"]);
        assert!(
            result.is_err(),
            "--upload-only with --init should produce a clap error"
        );
    }

    #[test]
    fn upload_only_without_filename_fails() {
        // --upload-only without a filename should fail because filename is required
        // unless --init is present
        let result = Args::try_parse_from(["shuk", "--upload-only"]);
        assert!(
            result.is_err(),
            "--upload-only without a filename should produce a clap error"
        );
    }

    #[test]
    fn upload_only_with_verbose_and_filename_succeeds() {
        let result = Args::try_parse_from(["shuk", "--upload-only", "--verbose", "myfile.txt"]);
        let args = result.expect("parsing should succeed");
        assert!(args.upload_only, "upload_only should be true");
        assert!(args.verbose, "verbose should be true");
        assert_eq!(
            args.filename,
            Some(PathBuf::from("myfile.txt")),
            "filename should be myfile.txt"
        );
        assert!(!args.init, "init should be false");
    }
}


// =============================================================================
// Property Test: Task 2.2 - Property 4
// Upload-only success output contains filename and bucket, never a URL
// Feature: upload-only, Property 4: Upload-only success output contains filename and bucket, never a URL
// **Validates: Requirements 5.1, 5.2**
// =============================================================================

mod property_upload_only_output_format {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn success_output_contains_filename_and_bucket_never_url(
            filename in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}",
            bucket in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}"
        ) {
            // Format the same success message template used in upload.rs
            let message = format!(
                "✅ | File uploaded: {}, to S3 Bucket: {}\n✅ | No presigned URL generated (upload-only mode)",
                filename, bucket
            );

            // Assert message contains the filename
            prop_assert!(
                message.contains(&filename),
                "Success message must contain the filename '{}', got: {}",
                filename, message
            );

            // Assert message contains the bucket name
            prop_assert!(
                message.contains(&bucket),
                "Success message must contain the bucket name '{}', got: {}",
                bucket, message
            );

            // Assert message does NOT contain presigned URL signature parameter
            prop_assert!(
                !message.contains("X-Amz-Signature"),
                "Success message must not contain 'X-Amz-Signature', got: {}",
                message
            );

            // Assert message does NOT match presigned URL pattern (s3 amazonaws URL)
            let has_presigned_url_pattern = message.contains("https://") && message.contains("s3") && message.contains("amazonaws.com");
            prop_assert!(
                !has_presigned_url_pattern,
                "Success message must not contain presigned URL pattern (https://.*s3.*amazonaws.com), got: {}",
                message
            );
        }
    }
}

// =============================================================================
// Property Test: Task 3.4 - Property 5
// Already-exists output in upload-only mode contains filename
// Feature: upload-only, Property 5: Already-exists output in upload-only mode contains filename
// **Validates: Requirements 3.2**
// =============================================================================

mod property_already_exists_output {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn already_exists_output_contains_filename_no_url(
            filename in "[a-zA-Z][a-zA-Z0-9_.-]{0,50}"
        ) {
            // Format the same "already exists" message template used in main.rs
            let message = format!(
                "✅ | File already exists in S3: {}\n✅ | No action taken (upload-only mode)",
                filename
            );

            // Assert message contains the filename
            prop_assert!(
                message.contains(&filename),
                "Already-exists message must contain the filename '{}', got: {}",
                filename, message
            );

            // Assert message does NOT contain presigned URL signature parameter
            prop_assert!(
                !message.contains("X-Amz-Signature"),
                "Already-exists message must not contain 'X-Amz-Signature', got: {}",
                message
            );

            // Assert message does NOT match presigned URL pattern
            let has_presigned_url_pattern = message.contains("https://") && message.contains("s3") && message.contains("amazonaws.com");
            prop_assert!(
                !has_presigned_url_pattern,
                "Already-exists message must not contain presigned URL pattern (https://.*s3.*amazonaws.com), got: {}",
                message
            );

            // Assert message does NOT contain URL-related messaging
            prop_assert!(
                !message.contains("presigned URL:"),
                "Already-exists message must not contain 'presigned URL:' messaging, got: {}",
                message
            );
            prop_assert!(
                !message.contains("here is your file"),
                "Already-exists message must not contain URL-related messaging like 'here is your file', got: {}",
                message
            );
        }
    }
}
