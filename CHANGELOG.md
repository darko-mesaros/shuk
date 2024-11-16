# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## TODO 📋

- TODO: Ability to delete objects
- TODO: Ability to archive objects
- TODO: Have the progress bar remain on screen, or show summary of upload.
- TODO: Have the ability to configure the chunk size for multi-part uploads
- TODO: User configurable tags

## [0.4.5] - 2024-07-31
### Added
- Shuk now checks if a file is already uploaded, and if it is it just presigns it again.

### Changed
- Cleaned up some code
- Fixed region selection during the SDK configuration

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
