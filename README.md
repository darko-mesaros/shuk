# Shuk 💾 ➡️ 🪣

![screenshot of shuk](/img/shuk.png)

⚠️**BETA SOFTWARE**⚠️

*Shuk* is used to upload files *of any size* to [Amazon S3](https://aws.amazon.com/s3/) and have them shared with others via a [presigned URL](https://docs.aws.amazon.com/AmazonS3/latest/userguide/ShareObjectPreSignedURL.html). If the same file already exists at the same location, it will **only presign it**.

## Installation 💾

To install this tool, make sure you have `rust` and `cargo` installed and run:
```
cargo install shuk
```

## Usage 🚀
```
Usage: shuk [OPTIONS] [FILENAME]

Arguments:
  [FILENAME]

Options:
      --init
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

- For the `use_clipboard` feature to compile on X11, you need the `xorg-dev` library.
