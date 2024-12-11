# Shuk 💾 ➡️ 🪣

![screenshot of shuk](/img/shuk.png)

⚠️**BETA SOFTWARE**⚠️

*Shuk* is used to upload files *of any size* to [Amazon S3](https://aws.amazon.com/s3/) and have them shared with others via a [presigned URL](https://docs.aws.amazon.com/AmazonS3/latest/userguide/ShareObjectPreSignedURL.html). If the same file already exists at the same location, it will **only presign it**.

## Installation 💾

To install this tool, make sure you have `rust` and `cargo` installed and run:
```
cargo install shuk
```

> **NOTE**: Whenever installing a new version, run `shuk --init` for best results. Sometimes new configuration options are added.

## Usage 🚀
```
Usage: shuk [OPTIONS] [FILENAME]

Arguments:
  [FILENAME]

Options:
      --init
  -v, --verbose  Enable verbose logging
  -h, --help     Print help
  -V, --version  Print version
```

Just pass the filename as the argument to `shuk`:
```bash
shuk filename.bla
```

## Configuration 🔧

All the configuration is located in the `$HOME/.config/shuk.shuk.toml` file. 

```toml
# The bucket name where the files will be uploaded
bucket_name = "alan-ford-bucket"
# The prefix (folder) for the uploads. Leave blank "" for the root of the bucket
bucket_prefix = "shuk"
# Length of time in seconds on how long will the presigned URL be valid for
presigned_time = 86400
# The AWS profile shuk will use
aws_profile = "default"
# Should the presigned URL be stored directly to the clipboard or not
use_clipboard = false
# Set the fallback region
fallback_region = "us-east-1"
```

To automatically configure this file just run `shuk --init`

## Build Notes

Check the `BUILDING.md` file in this repo.

## Troubleshooting

This project uses the [log](https://crates.io/crates/log) crate. To get different levels of logging set the `SHUK_LOG` environment variable to either `trace`, `warn`, `info`, `debug`, or `error`. By default it is using the `warn` level.

Or better yet, just pass the `--verbose` flag, as this will run the `trace` level output. Be careful, there will be a lot of stuff on your screen.
