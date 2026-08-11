# Shuk 💾 ➡️ 🪣

![screenshot of shuk](/img/shuk.png)

⚠️**BETA SOFTWARE**⚠️

*Shuk* is used to upload files *of any size* to [Amazon S3](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html) and have them shared with others via a [presigned URL](https://docs.aws.amazon.com/AmazonS3/latest/userguide/ShareObjectPreSignedURL.html). If the same file already exists at the same location, it will **only presign it**.

## Installation 💾

To install this tool, make sure you have `rust` and `cargo` installed and run:
```bash
cargo install shuk
```

> **NOTE**: Whenever installing a new version, run `shuk --init` for best results. Sometimes new configuration options are added.

## Usage 🚀

```text
Usage: shuk [OPTIONS] [FILENAME]

Arguments:
  [FILENAME]

Options:
      --init
      --upload-only  Upload without generating a presigned URL
  -v, --verbose      Enable verbose logging
  -h, --help         Print help
  -V, --version      Print version
```

Just pass the filename as the argument to `shuk`:
```bash
shuk filename.bla
```

## Configuration 🔧

All configuration is stored in `$HOME/.config/shuk/shuk.toml`.

```toml
# The bucket name where the files will be uploaded
bucket_name = "alan-ford-bucket"
# The prefix (folder) for the uploads. Leave blank "" for the root of the bucket
bucket_prefix = "shuk"
# Length of time in seconds for which the presigned URL will be valid
presigned_time = 86400
# The AWS profile Shuk will use. Omit this to use the default AWS credential chain
aws_profile = "default"
# Whether to copy the presigned URL directly to the clipboard
use_clipboard = false
# Initial region when the AWS profile or environment does not provide one
fallback_region = "us-east-1"
```

Shuk uses the standard AWS region provider chain. If Amazon S3 reports that the bucket is in a different region, Shuk retries once with that region and prints the setting you should update. It does not rewrite your configuration automatically.

To configure this file interactively, run `shuk --init`.

## Build Notes

Check `BUILDING.md` in this repository.

## Troubleshooting

Shuk treats only an S3 `NotFound` response as proof that an object is absent. Authorization failures, timeouts, and unrecognized service responses stop the upload rather than risking replacement of an object whose existence could not be checked.

Service errors include the operation, S3 URI, effective region, HTTP status, AWS error code and message when available, and the AWS request ID. Region mismatches are recovered automatically.

This project uses the [log](https://crates.io/crates/log) crate. Set `SHUK_LOG` to `trace`, `warn`, `info`, `debug`, or `error` to control logging. The default is `warn`.

Pass `--verbose` to enable trace logging. Be careful: trace output is intentionally detailed.
