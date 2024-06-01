# Shuk 💾 ➡️ 🪣

![screenshot of shuk](/img/shuk.png)

⚠️**ALPHA SOFTWARE**⚠️

*Shuk* is used to upload files to [Amazon S3](https://aws.amazon.com/s3/) and have them shared with others.

## Usage 🔧

The file `shuk.toml` needs to contain two bits of information: 
- The bucket name of the bucket you wish to upload to
- Expiration time of your presigned objects

Just pass the filename as the argument to `shuk`:
```bash
cargo run filename.bla
```
