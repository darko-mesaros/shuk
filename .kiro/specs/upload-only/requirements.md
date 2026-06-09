# Requirements Document

## Introduction

This document defines the requirements for the "upload-only" feature in the Shuk CLI tool. Currently, Shuk always generates a presigned URL after uploading a file to S3 (or re-presigns if the file already exists). The upload-only feature introduces a command-line flag that allows users to upload a file to S3 without generating a presigned URL afterward. This is useful for scenarios where the user simply wants to store a file in S3 without sharing it via a temporary link.

## Glossary

- **Shuk**: The Rust-based CLI tool that uploads files to Amazon S3 and generates presigned URLs for sharing.
- **CLI_Parser**: The argument parsing component of Shuk, implemented using the clap crate, responsible for interpreting command-line flags and arguments.
- **Upload_Engine**: The component of Shuk responsible for uploading files to S3, implemented in the `upload_object` function.
- **Presign_Generator**: The component that generates presigned URLs for uploaded S3 objects, implemented via the `presign_file` function.
- **Upload_Only_Flag**: The new command-line flag (`--upload-only`) that instructs Shuk to skip presigned URL generation after uploading.
- **Presigned_URL**: A time-limited URL that grants temporary access to download a private S3 object.
- **Just_Presign_Mode**: The existing behavior where Shuk detects that an identical file already exists in S3 and only regenerates a presigned URL without re-uploading.

## Requirements

### Requirement 1: Upload-Only CLI Flag Definition

**User Story:** As a user, I want to pass an `--upload-only` flag to Shuk, so that I can upload a file to S3 without generating a presigned URL.

#### Acceptance Criteria

1. THE CLI_Parser SHALL accept an optional `--upload-only` boolean flag that defaults to `false` when not provided.
2. WHEN the `--upload-only` flag is provided alongside a filename, THE CLI_Parser SHALL exit with code 0 and make both the filename and the `--upload-only` value available for downstream processing without writing to stderr.
3. WHEN the `--upload-only` flag is provided without a filename, THE CLI_Parser SHALL exit with a non-zero exit code and write an error message to stderr indicating that a filename is required.
4. WHEN the `--upload-only` flag is provided together with the `--init` flag, THE CLI_Parser SHALL exit with a non-zero exit code and write an error message to stderr indicating that these flags conflict.
5. WHEN the `--upload-only` flag is provided together with the `--verbose` flag and a filename, THE CLI_Parser SHALL parse all arguments without conflict and exit with code 0.

### Requirement 2: Skip Presigned URL Generation

**User Story:** As a user, I want Shuk to skip presigned URL generation when I use the upload-only flag, so that I can upload files purely for storage without producing a shareable link.

#### Acceptance Criteria

1. WHEN the `--upload-only` flag is provided and the file does not exist in S3, THE Upload_Engine SHALL upload the file to S3 without invoking the Presign_Generator, and THE Shuk SHALL not copy any value to the system clipboard.
2. WHEN the `--upload-only` flag is provided and the file already exists in S3 with matching content, THE Shuk SHALL display a confirmation message indicating the file already exists in storage and SHALL skip both the upload and presigned URL generation, and SHALL exit with a zero exit code.
3. WHEN the `--upload-only` flag is provided and the file upload completes successfully, THE Shuk SHALL display a confirmation message to stdout indicating the file was uploaded successfully without a presigned URL.
4. WHEN the `--upload-only` flag is provided and the file upload completes successfully, THE Shuk SHALL exit with a zero exit code.
5. IF the `--upload-only` flag is provided and the file upload fails, THEN THE Shuk SHALL display an error message to stderr indicating the upload failure reason and SHALL exit with a non-zero exit code.

### Requirement 3: Upload-Only Behavior With Existing Files

**User Story:** As a user, I want clear behavior when I use the upload-only flag and the same file already exists in S3, so that I understand what the tool did.

#### Acceptance Criteria

1. WHEN the `--upload-only` flag is provided and the remote file is determined to be identical (matching file size, start_hash, and end_hash from 8KB partial hash comparison), THE Shuk SHALL skip both the upload and presigned URL generation and exit with code 0.
2. WHEN the `--upload-only` flag is provided and the remote file is determined to be identical, THE Shuk SHALL display a message to stdout indicating the file name, that it already exists in S3, and that no action was taken.
3. WHEN the `--upload-only` flag is provided and a file with the same name but different content exists in S3 (differing file size or differing partial hashes), THE Shuk SHALL upload the new file to S3 and SHALL NOT generate a presigned URL.
4. IF the `--upload-only` flag is provided and the identity check against S3 fails (due to network error, missing tags, or inaccessible bucket), THEN THE Shuk SHALL display an error message to stderr indicating the failure reason and exit with a non-zero exit code without uploading the file.

### Requirement 4: Clipboard Behavior With Upload-Only

**User Story:** As a user, I want the clipboard to remain unmodified when using upload-only mode, so that my clipboard contents are not disrupted.

#### Acceptance Criteria

1. IF the `--upload-only` flag is provided, THEN THE Shuk SHALL not invoke any system clipboard utility (pbcopy, xclip, wl-copy, or clip.exe) regardless of the `use_clipboard` configuration setting.
2. IF the `--upload-only` flag is provided, THEN THE Shuk SHALL upload the file to S3 without generating or displaying a presigned URL.
3. IF the `--upload-only` flag is provided and the upload completes successfully, THEN THE Shuk SHALL display a confirmation message indicating the file was uploaded.

### Requirement 5: Upload-Only Output and Logging

**User Story:** As a user, I want clear terminal output when using upload-only mode, so that I can confirm the upload succeeded without confusion about a missing URL.

#### Acceptance Criteria

1. WHEN the `--upload-only` flag is provided and the upload succeeds, THE Shuk SHALL print a success message to stdout that includes the uploaded file name and the destination bucket name, and SHALL exit with code 0.
2. WHEN the `--upload-only` flag is provided and the upload succeeds, THE Shuk SHALL NOT print a presigned URL, URL-related messaging, or any prompt referencing a shareable link.
3. WHEN the `--upload-only` flag is provided and the `--verbose` flag is also provided, THE Shuk SHALL log trace-level messages indicating that presigned URL generation was skipped.
4. IF the upload fails while the `--upload-only` flag is provided, THEN THE Shuk SHALL print an error message to stderr indicating the failure reason and exit with a non-zero exit code.
5. WHEN the `--upload-only` flag is provided and the upload succeeds, THE Shuk SHALL skip clipboard operations regardless of the `use_clipboard` configuration setting.

### Requirement 6: Backward Compatibility

**User Story:** As an existing user, I want Shuk to behave exactly as before when I do not use the upload-only flag, so that my existing workflows are not disrupted.

#### Acceptance Criteria

1. WHEN the `--upload-only` flag is not provided and a filename argument is supplied, THE Shuk SHALL upload the file to S3, generate a presigned URL valid for the configured `presigned_time` seconds, and display the presigned URL to standard output.
2. IF the `--upload-only` flag is not provided and the file already exists in S3 with matching start and end partial hashes, THEN THE Shuk SHALL skip the upload and only regenerate a presigned URL valid for the configured `presigned_time` seconds.
3. WHEN the `--upload-only` flag is not provided and `use_clipboard` is set to true in the configuration, THE Shuk SHALL copy the presigned URL to the system clipboard after displaying it to standard output.
4. WHEN the `--upload-only` flag is not provided and the file size exceeds 4 GB, THE Shuk SHALL upload the file using multipart upload with a progress bar displayed to the user during the transfer.
5. THE Shuk SHALL preserve the existing behavior of the `--init`, `--verbose`, and filename arguments regardless of whether the `--upload-only` flag is defined in the argument parser.
