# Shuk 💾 ➡️ 🪣

![screenshot of shuk](/img/shuk.png)

⚠️**BETA SOFTWARE**⚠️

*Shuk* is used to upload files *of any size* to [Amazon S3](https://aws.amazon.com/s3/) and have them shared with others via a [presigned URL](https://docs.aws.amazon.com/AmazonS3/latest/userguide/ShareObjectPreSignedURL.html).

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
# Lenght of time in seconds on how long will the presigned URL be valid for
presigned_time = 86400
# The AWS profile shuk will use
aws_profile = "default"
```

To automatically configure this file just run `shuk --init`
