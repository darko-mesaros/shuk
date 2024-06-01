# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## TODO 📋

- TODO: Presign files so we can share
- TODO: Ability to delete objects
- TODO: Ability to archive objects
- TODO: Have the progress bar remain on screen, or show summary of upload.
- TODO: Install the configuration files in the users `.config` directory
- TODO: Clean up the upload function so it one function instead of two.
- TODO: Have the ability to configure the chunk size for multi-part uploads

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
