# Implementation Plan: Upload-Only Feature

## Overview

This plan implements the `--upload-only` CLI flag for Shuk. The implementation modifies three source files: `src/utils.rs` (CLI parsing), `src/upload.rs` (upload logic), and `src/main.rs` (main flow orchestration). The changes add a new boolean flag to the argument parser, modify `upload_object` to conditionally skip presigning, and add a new code path in `main` that handles the upload-only flow including the "already exists" short-circuit.

## Tasks

- [x] 1. Add `--upload-only` flag to CLI argument parser
  - [x] 1.1 Add `upload_only` field to the `Args` struct in `src/utils.rs`
    - Add a new boolean field `upload_only` to the `Args` struct with clap attributes: `#[arg(long, help = "Upload file without generating a presigned URL", conflicts_with("init"))]`
    - The field must default to `false` when not provided
    - The `conflicts_with("init")` attribute ensures `--upload-only` and `--init` cannot be used together
    - The existing `required_unless_present("init")` on `filename` ensures `--upload-only` without a filename produces a clap error
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5_

  - [ ]* 1.2 Write property test for CLI argument parsing (Property 1)
    - **Property 1: Upload-only flag parses correctly with any valid filename**
    - Add `proptest` as a dev-dependency in `Cargo.toml`
    - Create a test module in `src/utils.rs` (or a separate test file) that uses `proptest` to generate random valid filename strings and verifies that `Args::try_parse_from(["shuk", "--upload-only", &filename])` succeeds with `upload_only == true` and the filename set correctly
    - **Validates: Requirements 1.2, 1.5**

  - [ ]* 1.3 Write property test for backward compatibility (Property 2)
    - **Property 2: Existing argument combinations remain valid**
    - Generate random valid filenames and verify that parsing without `--upload-only` produces `upload_only == false`
    - Verify that `--init` alone still parses correctly with `upload_only == false`
    - Verify that filename + `--verbose` still parses correctly with `upload_only == false`
    - **Validates: Requirements 6.5**

  - [ ]* 1.4 Write unit tests for `--upload-only` flag conflict and error cases
    - Test that `--upload-only` with `--init` fails with a clap error
    - Test that `--upload-only` without a filename fails with a clap error
    - Test that `--upload-only` with `--verbose` and a filename succeeds
    - _Requirements: 1.3, 1.4, 1.5_

- [x] 2. Modify `upload_object` to support upload-only mode
  - [x] 2.1 Change `upload_object` return type and add `upload_only` parameter in `src/upload.rs`
    - Change the function signature to accept a new `upload_only: bool` parameter
    - Change the return type from `Result<String, anyhow::Error>` to `Result<Option<String>, anyhow::Error>`
    - When `upload_only` is `true`: after upload completes (both single-part and multi-part paths), skip the `presign_file` call, print the upload-only success message (`"✅ | File uploaded: {key}, to S3 Bucket: {bucket}\n✅ | No presigned URL generated (upload-only mode)"`), and return `Ok(None)`
    - When `upload_only` is `false`: maintain existing behavior but wrap the presigned URL in `Some(...)`, returning `Ok(Some(presigned_url))`
    - When `just_presign` is `true` and `upload_only` is `false`: maintain existing presign-only behavior, returning `Ok(Some(presigned_url))`
    - When `just_presign` is `true` and `upload_only` is `true`: this case should not occur in practice (handled in main), but for safety return `Ok(None)` with appropriate messaging
    - _Requirements: 2.1, 2.3, 2.4, 5.1, 5.2_

  - [ ]* 2.2 Write property test for upload-only output format (Property 4)
    - **Property 4: Upload-only success output contains filename and bucket, never a URL**
    - This can be tested by verifying the format of the success message string generated for any random filename/bucket pair
    - Generate random filename and bucket name strings, format the success message, assert it contains both the filename and bucket name, and does not contain `X-Amz-Signature` or presigned URL patterns
    - **Validates: Requirements 5.1, 5.2**

- [x] 3. Implement upload-only flow in main function
  - [x] 3.1 Add upload-only early exit for identical files in `src/main.rs`
    - After the existing `just_upload` determination (file exists and matches), if `arguments.upload_only` is `true` and `just_upload` is `true`: print the "already exists" message (`"✅ | File already exists in S3: {filename}\n✅ | No action taken (upload-only mode)"`) and exit with code 0 without calling `upload_object`
    - If `arguments.upload_only` is `true` and the S3 existence check failed (the `Err(e)` branch), print the error to stderr and exit with code 1 without attempting upload
    - _Requirements: 2.2, 3.1, 3.2, 3.4_

  - [x] 3.2 Wire `upload_only` flag through to `upload_object` call in `src/main.rs`
    - Pass `arguments.upload_only` as the new `upload_only` parameter to `upload_object`
    - Update the match on `upload_object`'s return value to handle `Ok(Some(url))` and `Ok(None)`:
      - `Ok(Some(presigned_url))`: existing behavior — optionally copy to clipboard if `use_clipboard` is true
      - `Ok(None)`: upload-only mode succeeded — skip clipboard operations entirely
    - _Requirements: 2.1, 4.1, 4.2, 4.3, 5.5_

  - [x] 3.3 Add verbose logging for upload-only mode in `src/main.rs`
    - When `arguments.upload_only` is `true` and `arguments.verbose` is `true`, emit a `log::trace!` message indicating that presigned URL generation was skipped due to upload-only mode
    - Add trace logging at the point where the upload-only early exit occurs (file already exists) and at the point where clipboard is skipped
    - _Requirements: 5.3_

  - [ ]* 3.4 Write property test for already-exists output (Property 5)
    - **Property 5: Already-exists output in upload-only mode contains filename**
    - Generate random filenames, format the "already exists" message, assert it contains the filename and does not contain presigned URL patterns or URL-related messaging
    - **Validates: Requirements 3.2**

- [x] 4. Checkpoint - Verify all changes compile and tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Integration testing and backward compatibility verification
  - [ ]* 5.1 Write integration tests for upload-only mode
    - Create integration test file `tests/upload_only_test.rs`
    - Test CLI argument parsing in isolation: verify `--upload-only` with filename, without filename (error), with `--init` (error), with `--verbose`
    - Test that default mode (no `--upload-only`) still produces `upload_only == false`
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 6.1, 6.2, 6.3, 6.5_

  - [ ]* 5.2 Write integration tests for clipboard suppression (Property 3)
    - **Property 3: Clipboard is never invoked in upload-only mode**
    - Verify that when `upload_object` returns `Ok(None)` (upload-only success), the code path that calls `set_into_clipboard` is never reached regardless of the `use_clipboard` config value
    - This can be a unit test of the main logic or an integration test that captures stdout/stderr
    - **Validates: Requirements 4.1, 5.5**

- [x] 6. Final checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- The implementation language is Rust, matching the existing codebase and design document
- The `proptest` crate is required as a dev-dependency for property-based tests
- No new runtime dependencies are needed — only existing crates (clap, aws-sdk-s3, colored) are used

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1"] },
    { "id": 1, "tasks": ["1.2", "1.3", "1.4", "2.1"] },
    { "id": 2, "tasks": ["2.2", "3.1", "3.2"] },
    { "id": 3, "tasks": ["3.3", "3.4"] },
    { "id": 4, "tasks": ["5.1", "5.2"] }
  ]
}
```
