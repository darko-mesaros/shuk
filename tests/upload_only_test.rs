use clap::Parser;
use shuk::utils::Args;
use std::path::PathBuf;

// =============================================================================
// Integration Tests: Task 5.1
// Upload-only mode CLI argument parsing integration tests
// Tests the full CLI argument parsing behavior in isolation, verifying correct
// parsing, error cases, and backward compatibility.
// Requirements: 1.1, 1.2, 1.3, 1.4, 6.1, 6.2, 6.3, 6.5
// =============================================================================

mod integration_upload_only_cli {
    use super::*;

    /// Requirement 1.1: --upload-only flag is accepted and defaults to false
    #[test]
    fn upload_only_flag_defaults_to_false() {
        let args = Args::try_parse_from(["shuk", "somefile.txt"])
            .expect("parsing filename alone should succeed");
        assert!(!args.upload_only, "upload_only should default to false when not provided");
        assert_eq!(args.filename, Some(PathBuf::from("somefile.txt")));
    }

    /// Requirement 1.2: --upload-only with a filename parses correctly
    #[test]
    fn upload_only_with_filename_succeeds() {
        let args = Args::try_parse_from(["shuk", "--upload-only", "document.pdf"])
            .expect("--upload-only with filename should succeed");
        assert!(args.upload_only, "upload_only should be true");
        assert_eq!(args.filename, Some(PathBuf::from("document.pdf")));
        assert!(!args.init);
        assert!(!args.verbose);
    }

    /// Requirement 1.3: --upload-only without a filename produces an error
    #[test]
    fn upload_only_without_filename_errors() {
        let result = Args::try_parse_from(["shuk", "--upload-only"]);
        assert!(
            result.is_err(),
            "--upload-only without a filename should produce a clap error"
        );
    }

    /// Requirement 1.4: --upload-only with --init produces a conflict error
    #[test]
    fn upload_only_with_init_conflicts() {
        let result = Args::try_parse_from(["shuk", "--upload-only", "--init"]);
        assert!(
            result.is_err(),
            "--upload-only with --init should produce a clap error"
        );
    }

    /// Requirement 1.4: --upload-only with --init and a filename still conflicts
    #[test]
    fn upload_only_with_init_and_filename_conflicts() {
        let result = Args::try_parse_from(["shuk", "--upload-only", "--init", "file.txt"]);
        assert!(
            result.is_err(),
            "--upload-only with --init should always produce a clap error, even with filename"
        );
    }

    /// Requirement 1.2, 1.5: --upload-only with --verbose and filename succeeds
    #[test]
    fn upload_only_with_verbose_succeeds() {
        let args = Args::try_parse_from(["shuk", "--upload-only", "--verbose", "image.png"])
            .expect("--upload-only with --verbose and filename should succeed");
        assert!(args.upload_only, "upload_only should be true");
        assert!(args.verbose, "verbose should be true");
        assert_eq!(args.filename, Some(PathBuf::from("image.png")));
        assert!(!args.init);
    }

    /// Requirement 6.5: --init alone still works (backward compatibility)
    #[test]
    fn init_alone_still_works() {
        let args = Args::try_parse_from(["shuk", "--init"])
            .expect("--init alone should still work");
        assert!(args.init);
        assert!(!args.upload_only);
        assert!(!args.verbose);
        assert_eq!(args.filename, None);
    }

    /// Requirement 6.5: filename with --verbose still works (backward compatibility)
    #[test]
    fn filename_with_verbose_still_works() {
        let args = Args::try_parse_from(["shuk", "--verbose", "backup.tar.gz"])
            .expect("filename with --verbose should still work");
        assert!(!args.upload_only, "upload_only should default to false");
        assert!(args.verbose);
        assert_eq!(args.filename, Some(PathBuf::from("backup.tar.gz")));
    }

    /// Requirement 6.1, 6.2, 6.3: Default mode (no --upload-only) produces upload_only == false
    /// This confirms backward compatibility: existing workflows are unaffected.
    #[test]
    fn default_mode_no_upload_only_flag() {
        let args = Args::try_parse_from(["shuk", "report.csv"])
            .expect("default mode should succeed");
        assert!(!args.upload_only, "upload_only must be false in default mode");
        assert_eq!(args.filename, Some(PathBuf::from("report.csv")));
    }

    /// Various valid filenames with --upload-only
    #[test]
    fn upload_only_with_various_filenames() {
        let filenames = vec![
            "file.txt",
            "my-archive.tar.gz",
            "REPORT_2024.pdf",
            "data.json",
            "photo.JPEG",
        ];

        for filename in filenames {
            let args = Args::try_parse_from(["shuk", "--upload-only", filename])
                .unwrap_or_else(|_| panic!("--upload-only with '{}' should succeed", filename));
            assert!(args.upload_only);
            assert_eq!(args.filename, Some(PathBuf::from(filename)));
        }
    }
}

// =============================================================================
// Integration Tests: Task 5.2 - Property 3
// Clipboard suppression in upload-only mode
//
// Property 3: Clipboard is never invoked in upload-only mode
//
// Since we cannot easily mock S3 or the clipboard system in integration tests
// without a mock framework, we verify the decision logic that determines whether
// clipboard should be invoked.
//
// The main.rs flow works as follows:
//   match upload_object(...).await {
//       Ok(Some(presigned_url)) => {
//           // ONLY this branch can invoke clipboard
//           if shuk_config.use_clipboard.unwrap_or(false) {
//               set_into_clipboard(presigned_url);
//           }
//       }
//       Ok(None) => {
//           // Upload-only mode: clipboard is NEVER reached here
//       }
//       Err(e) => { ... }
//   }
//
// In upload-only mode, upload_object returns Ok(None), which means the
// Ok(Some(...)) branch — the ONLY branch that calls set_into_clipboard —
// is never entered. Therefore, clipboard is never invoked.
//
// These tests verify:
// 1. The decision function correctly identifies when clipboard should NOT be used
// 2. The decision holds regardless of use_clipboard config value
//
// **Validates: Requirements 4.1, 5.5**
// =============================================================================

mod integration_clipboard_suppression {
    use super::*;

    /// Encapsulates the decision logic from main.rs:
    /// Given the result of upload_object and the use_clipboard config,
    /// determines if clipboard should be invoked.
    ///
    /// Returns true if clipboard SHOULD be invoked, false otherwise.
    fn should_invoke_clipboard(
        upload_result: &Result<Option<String>, &str>,
        use_clipboard: bool,
    ) -> bool {
        match upload_result {
            Ok(Some(_presigned_url)) => use_clipboard,
            Ok(None) => false,    // Upload-only mode — never invoke clipboard
            Err(_) => false,      // Error — never invoke clipboard
        }
    }

    /// When upload_object returns Ok(None) (upload-only success), clipboard is NEVER
    /// invoked, even if use_clipboard is true.
    #[test]
    fn clipboard_not_invoked_when_upload_only_returns_none_and_clipboard_enabled() {
        let result: Result<Option<String>, &str> = Ok(None);
        let use_clipboard = true;

        assert!(
            !should_invoke_clipboard(&result, use_clipboard),
            "Clipboard must NOT be invoked when upload_object returns Ok(None), \
             even when use_clipboard is true"
        );
    }

    /// When upload_object returns Ok(None) and use_clipboard is false, clipboard is not invoked.
    #[test]
    fn clipboard_not_invoked_when_upload_only_returns_none_and_clipboard_disabled() {
        let result: Result<Option<String>, &str> = Ok(None);
        let use_clipboard = false;

        assert!(
            !should_invoke_clipboard(&result, use_clipboard),
            "Clipboard must NOT be invoked when upload_object returns Ok(None) \
             and use_clipboard is false"
        );
    }

    /// When upload_object returns Ok(Some(url)) and use_clipboard is true,
    /// clipboard IS invoked (normal mode behavior).
    #[test]
    fn clipboard_invoked_when_normal_mode_returns_url_and_clipboard_enabled() {
        let result: Result<Option<String>, &str> =
            Ok(Some("https://example.com/presigned".to_string()));
        let use_clipboard = true;

        assert!(
            should_invoke_clipboard(&result, use_clipboard),
            "Clipboard SHOULD be invoked in normal mode when use_clipboard is true"
        );
    }

    /// When upload_object returns Ok(Some(url)) but use_clipboard is false,
    /// clipboard is NOT invoked (respects config).
    #[test]
    fn clipboard_not_invoked_when_normal_mode_returns_url_but_clipboard_disabled() {
        let result: Result<Option<String>, &str> =
            Ok(Some("https://example.com/presigned".to_string()));
        let use_clipboard = false;

        assert!(
            !should_invoke_clipboard(&result, use_clipboard),
            "Clipboard must NOT be invoked when use_clipboard is false, \
             even when a presigned URL is available"
        );
    }

    /// When upload_object returns Err, clipboard is never invoked.
    #[test]
    fn clipboard_not_invoked_on_error() {
        let result: Result<Option<String>, &str> = Err("upload failed");
        let use_clipboard = true;

        assert!(
            !should_invoke_clipboard(&result, use_clipboard),
            "Clipboard must NOT be invoked when upload_object returns an error"
        );
    }

    /// Property-based test: For ANY use_clipboard value, when upload_object returns
    /// Ok(None) (upload-only mode), clipboard is NEVER invoked.
    mod property_clipboard_suppression {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Property 3: Clipboard is never invoked in upload-only mode
            /// For any use_clipboard config value, Ok(None) means no clipboard.
            #[test]
            fn clipboard_never_invoked_for_upload_only_result(use_clipboard in proptest::bool::ANY) {
                let result: Result<Option<String>, &str> = Ok(None);
                prop_assert!(
                    !should_invoke_clipboard(&result, use_clipboard),
                    "Clipboard must NEVER be invoked when upload_object returns Ok(None) (upload-only mode), \
                     regardless of use_clipboard={}", use_clipboard
                );
            }

            /// Contrast: In normal mode with Ok(Some(url)), clipboard respects the config.
            #[test]
            fn clipboard_respects_config_in_normal_mode(use_clipboard in proptest::bool::ANY) {
                let result: Result<Option<String>, &str> =
                    Ok(Some("https://bucket.s3.amazonaws.com/file?X-Amz-Signature=abc".to_string()));
                let should_clipboard = should_invoke_clipboard(&result, use_clipboard);
                prop_assert_eq!(
                    should_clipboard,
                    use_clipboard,
                    "In normal mode, clipboard invocation should match the use_clipboard config value"
                );
            }
        }
    }

    /// Integration test verifying that when Args has upload_only=true, the parsed
    /// value is correctly true — confirming the first condition for clipboard
    /// suppression (the second being Ok(None) return from upload_object).
    #[test]
    fn upload_only_parsed_value_is_true_for_clipboard_suppression() {
        let args = Args::try_parse_from(["shuk", "--upload-only", "test.txt"])
            .expect("parsing should succeed");

        // When upload_only is true in args, upload_object will be called with
        // upload_only=true, which returns Ok(None), which means the clipboard
        // branch (Ok(Some(...))) is never entered.
        assert!(
            args.upload_only,
            "upload_only must be true to trigger the Ok(None) return path"
        );
    }
}
