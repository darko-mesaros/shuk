# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.9] - 2026-08-11

### Added
- Automatically detect an S3 bucket's actual Region and retry once when the configured Region is wrong
- Include the S3 operation, URI, Region, HTTP status, AWS error details, and request ID in service errors

### Changed
- Updated the AWS SDK and Smithy dependencies to their latest compatible versions
- Reworked GitHub release automation to create releases from version tag pushes and upload binaries with SHA-256 checksums

### Fixed
- Stop uploads when Shuk cannot safely determine whether an object already exists
- Return actionable multipart upload errors instead of panicking on stream, ETag, or completion failures

## [0.4.8] - 2026-03-07

### Changed
- Updated all dependencies to latest compatible versions
- Reworked AWS SDK configuration to use native profile loading, enabling full profile support including `endpoint_url` — this allows shuk to work with S3-compatible APIs (e.g. Nebius Cloud)

## [0.4.7] - 2024-12-10

- TODO: Ability to delete objects
- TODO: Ability to archive objects
- TODO: Have the progress bar remain on screen, or show summary of upload.
- TODO: Have the ability to configure the chunk size for multi-part uploads
- TODO: User configurable tags

### Changed
- Reworked the way copy to clipboard works. Now we use native tools within the OS (`xclip`, `pbcopy`, `clip.exe`)
- Configured the build to be statically linked

## [0.4.6] - 2024-11-17
### Added
- Shuk now checks if a file is already uploaded, and if it is it just presigns it again.
- Proper tracing and logging using the log crate

### Changed
- Cleaned up some code
- Updated the AWS Crates to latest
- Fixed region selection during the SDK configuration - MAKE SURE TO UPDATE YOUR CONFIG FILE

## [0.4.4] - 2024-07-31
### Changed
- Improved some error handling

## [0.4.2] - 2024-07-14
### Added
- Ability to directly save the presigned URL to the system clipboard

## [0.4.1] - 2024-06-10
### Changed
- Improved the way we read and write the AWS Profile
- Fixed the way we write to the `shuk.toml` config file

### Thanks <3
- kaumnen
- noc7c9
- Research_DEV

## [0.4.0] - 2024-06-01
### Added
- The tool is now able to be installed and configured locally.
- You can run `--init` to set up the local configuration file in `~/.config/shuk`


## [0.3.1] - 2024-06-01
### Added
- AWS Profile selection from the config file
- The uploaded objects are now tagged with `deployed_by:shuk`
- Added the ability to define a prefix (folder) where to upload the files

### Changed
- Uses the AWS region from the profile first, then falls back to `us-west-2`
- Cleaned up the upload function, now its only a single one with the logic inside.
- Improved the path handling (works with non UTF-8 characters)

## [0.3.0] - 2024-05-31
### Added
- Can upload files larger than 5GB (thanks to multi-part uploads)

## [0.2.0] - 2024-03-12
### Added
- Can now parse filename from arguments
- We have a configuration file for bucket name
- Can presign file when uploaded.

## [0.1.0] - 2024-03-11
### Added
- Basic functionality
- Uploads fixed files to fixed buckets
