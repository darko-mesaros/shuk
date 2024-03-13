# Shuk

**ALPHA SOFTWARE**

*Shuk* is used to upload files to [Amazon S3](https://aws.amazon.com/s3/) and have them shared with others.

## Usage 🔧

The file `shuk.toml` needs to contain two bits of information: 
- The bucket name of the bucket you wish to upload to
- Expiration time of your presigned objects

Just pass the filename as the argument to `shuk`:
```bash
cargo run filename.bla
```

## TODO 📋

- [x] Presign files so we can share
- [ ] Ability to delete objects
- [ ] Ability to archive objects
- [ ] Have the progress bar remain on screen, or show summary of upload.
- [ ] Install the configuration files in the users `.config` directory

## Version Log 📜

### 0.0.1

- Basic functionality
- Uploads fixed files to fixed buckets

### 0.2.0

- Can now parse filename from arguments
- We have a configuration file for bucket name
- Can presign file when uploaded.
