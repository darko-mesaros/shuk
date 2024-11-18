use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client;
use md5;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::{path::Path, time::Duration};

#[derive(Debug)]
pub struct ObjectTags {
    pub managed_by: String,
    pub start_hash: String,
    pub end_hash: String,
}

// This converst the Struct into a list of tags the way the API accepts it
// NOTE: Maybe I can create a dedicated function for this?
impl std::fmt::Display for ObjectTags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "managed_by={}&start_hash={}&end_hash={}",
            self.managed_by, self.start_hash, self.end_hash
        )
    }
}

// Needed for the tagging
impl From<&ObjectTags> for String {
    fn from(tags: &ObjectTags) -> String {
        tags.to_string() // This uses your Display implementation
    }
}

pub async fn presign_file(
    client: &Client,
    bucket_name: &str,
    key: &str,
    prefix: Option<String>,
    presigned_time: u64,
) -> Result<String, anyhow::Error> {
    log::trace!(
        "Presigning file: {:?}/{} in bucket {} for duration of {}",
        &prefix,
        &key,
        &bucket_name,
        &presigned_time
    );
    let expires_in = Duration::from_secs(presigned_time);
    log::trace!("Sending get_object request that will presing the file");
    let presigned_request = client
        .get_object()
        .bucket(bucket_name)
        .key(format!("{}{}", prefix.unwrap_or("".into()), key))
        .presigned(PresigningConfig::expires_in(expires_in)?)
        .await?;

    Ok(presigned_request.uri().to_string())
}

pub fn calculate_file_md5<P: AsRef<Path>>(path: P) -> Result<String, anyhow::Error> {
    // Open and read the entire file
    let mut file = File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    // Calculate MD5
    let digest = md5::compute(&contents);

    // Convert to hex string
    Ok(format!("{:x}", digest))
}

// Just used to store the partial file hash
#[derive(Debug)]
pub struct PartialFileHash {
    pub start_hash: String,
    pub end_hash: String,
    pub file_size: u64,
}

pub fn calculate_partial_hash(local_path: &Path) -> Result<PartialFileHash, anyhow::Error> {
    log::trace!("Calculating partial hash of {:?}", &local_path);
    const SAMPLE_SIZE: usize = 8192;

    let mut file = File::open(local_path)?;
    let file_size = file.metadata()?.len();
    log::trace!("File size of {:?} is {:?}", &local_path, &file_size);

    let mut start_buffer = vec![0; SAMPLE_SIZE];
    let start_bytes_read = file.read(&mut start_buffer)?;
    start_buffer.truncate(start_bytes_read);
    let start_hash = format!("{:x}", md5::compute(&start_buffer));
    log::trace!("Start hash of {:?} is {:?}", &local_path, &start_hash);

    // This is just a check if the file is too small (less than 8KB as is the sample size by
    // default).
    // This should not happen, but is here just in case.
    let end_hash = if file_size > SAMPLE_SIZE as u64 {
        // Move to the end of file - SAMPLE_SIZE
        file.seek(SeekFrom::End(-(SAMPLE_SIZE as i64)))?;
        let mut end_buffer = vec![0; SAMPLE_SIZE];
        let end_bytes_read = file.read(&mut end_buffer)?;
        end_buffer.truncate(end_bytes_read);
        format!("{:x}", md5::compute(&end_buffer))
    } else {
        // The file is too small just use the start_hash
        log::warn!("The file seems to be smaller than the sample size for hashing. Not sure how we got here.");
        start_hash.clone()
    };
    log::trace!("End hash of {:?} is {:?}", &local_path, &end_hash);

    Ok(PartialFileHash {
        start_hash,
        end_hash,
        file_size,
    })
}

pub async fn file_exists_in_s3(
    client: &Client,
    bucket: &str,
    key: &str,
) -> Result<bool, anyhow::Error> {
    log::trace!("Testing if {:?} exists in bucket {:?}", &key, &bucket);
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => {
            log::trace!("File {:?} has been found in bucket {:?}", &key, &bucket);
            Ok(true)
        }
        Err(err) => {
            match err {
                SdkError::ServiceError(err) => {
                    match err.err() {
                        // If the error NotFound is returned - return false
                        HeadObjectError::NotFound(_) => {
                            log::trace!("File {:?} not found in bucket {:?}", &key, &bucket);
                            Ok(false)
                        }
                        other_err => {
                            log::warn!("There was a service error when checking for file {:?} in bucket {:?}", &key, &bucket);
                            Err(anyhow::anyhow!("S3 service error: {:?}", other_err))
                        }
                    }
                }
                other_err => {
                    log::warn!(
                        "There was a SDK when checking for file {:?} in bucket {:?}",
                        &key,
                        &bucket
                    );
                    Err(anyhow::anyhow!("S3 SDK error: {:?}", other_err))
                }
            }
        }
    }
}

// If you need metadata version:
async fn get_file_metadata(
    client: &Client,
    bucket: &str,
    key: &str,
) -> Result<Option<aws_sdk_s3::operation::head_object::HeadObjectOutput>, anyhow::Error> {
    log::trace!("Getting file metadata for {}:{}", &bucket, &key);
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(output) => {
            log::trace!(
                "Metadata has been extracted for {:?} in bucket {:?}",
                &key,
                &bucket
            );
            Ok(Some(output))
        }
        Err(err) => {
            match err {
                SdkError::ServiceError(err) => match err.err() {
                    HeadObjectError::NotFound(_) => {
                        log::warn!("Unable to find {:?} in bucket {:?}, there seems to be an issue getting metadata", &key, &bucket);
                        Ok(None)
                    }
                    other_err => {
                        log::warn!("There was a service error when gettinf metadata for file {:?} in bucket {:?}", &key, &bucket);
                        Err(anyhow::anyhow!("S3 service error: {:?}", other_err))
                    }
                },
                other_err => {
                    log::warn!("There was a S3 SDK error when gettinf metadata for file {:?} in bucket {:?}", &key, &bucket);
                    Err(anyhow::anyhow!("S3 SDK error: {:?}", other_err))
                }
            }
        }
    }
}

// If you need metadata version:
async fn get_file_tags(
    client: &Client,
    bucket: &str,
    key: &str,
) -> Result<Option<aws_sdk_s3::operation::get_object_tagging::GetObjectTaggingOutput>, anyhow::Error>
{
    log::trace!("Getting file tags for {}:{}", &bucket, &key);
    match client
        .get_object_tagging()
        .bucket(bucket)
        .key(key)
        .send()
        .await
    {
        Ok(output) => {
            log::trace!(
                "Tags for {} in {} were succesfully retrieved.",
                &key,
                &bucket
            );
            Ok(Some(output))
        }
        Err(err) => {
            log::warn!(
                "There was a S3 Service error when getting tags for {} in {}.",
                &key,
                &bucket
            );
            Err(anyhow::anyhow!("S3 service error: {:?}", err))
        }
    }
}

pub async fn quick_compare(
    local_path: &Path,
    bucket_name: &str,
    key: &str,
    local_object_tags: &ObjectTags,
    c: &Client,
) -> Result<bool, anyhow::Error> {
    log::trace!(
        "Comparing local and remote files: {:?} and {}/{}",
        &local_path,
        &bucket_name,
        &key
    );
    // Get file metadata
    let file = File::open(local_path)?;
    let file_size = file.metadata()?.len();
    log::trace!("Local file {:?} size: {:?}", &local_path, &file_size);

    let object_metadata = get_file_metadata(c, bucket_name, key).await?;
    log::trace!(
        "Remote file {}{} metadata: {:#?}",
        &bucket_name,
        &key,
        &object_metadata
    );
    let object_tags = get_file_tags(c, bucket_name, key).await?;
    log::trace!(
        "Remote file {}{} tags: {:#?}",
        &bucket_name,
        &key,
        &object_metadata
    );

    // NOTE: Very complex way of making sure the length of my remote file is extracted
    // if I cannot do it, I just return 0 and we reupload
    let s3_object_len = match object_metadata {
        None => {
            println!("I was unable to determine the file size of the remote object, something went wrong, we are uploading it again");
            0
        }
        Some(metadata) => match metadata.content_length() {
            None => {
                println!("I was unable to determine the file size of the remote object, something went wrong, we are uploading it again");
                0
            }
            Some(len) => match len.try_into() {
                Ok(size) => size,
                Err(_) => {
                    println!("I was unable to determine the file size of the remote object, something went wrong, we are uploading it again");
                    0
                }
            },
        },
    };
    log::trace!(
        "The size of the remote file {}{}: {}",
        &bucket_name,
        &key,
        &s3_object_len
    );

    // Extracting the hash tags
    // FIX: Clean it up
    let tags = object_tags.unwrap();
    let remote_start_hash = tags
        .tag_set()
        .iter()
        .find(|tag| tag.key() == "start_hash")
        .map(|tag| tag.value())
        .unwrap_or_default();
    log::trace!(
        "Remote file {}{} start_hash: {}",
        &bucket_name,
        &key,
        &remote_start_hash
    );

    let remote_end_hash = tags
        .tag_set()
        .iter()
        .find(|tag| tag.key() == "end_hash")
        .map(|tag| tag.value())
        .unwrap_or_default();
    log::trace!(
        "Remote file {}{} end_hash: {}",
        &bucket_name,
        &key,
        &remote_end_hash
    );

    // Compare the file size
    if file_size == s3_object_len {
        // If Same
        //   Compare partial hash
        if local_object_tags.start_hash == remote_start_hash
            && local_object_tags.end_hash == remote_end_hash
        {
            //   If the same - presign
            log::trace!("Both file are the same: local_object_tags.start_hash = {} == remote_start_hash {}; local_object_tags.end_hash = {} == remote_end_hash = {} ", &local_object_tags.start_hash, &remote_start_hash, &local_object_tags.end_hash, &remote_end_hash);
            Ok(true)
        } else {
            log::trace!("The filenames are the same, but their partial hashes differ: local_object_tags.start_hash = {} != remote_start_hash {}; local_object_tags.end_hash = {} != remote_end_hash = {} ", &local_object_tags.start_hash, &remote_start_hash, &local_object_tags.end_hash, &remote_end_hash);
            println!("⚠️ | There seems to be a file with the same filename already at the destination. They are, also, the same sizes. HOWEVER, their partial hashes differ. I will assume that that they are different, so I will upload this one");
            Ok(false)
        }
    } else {
        log::trace!(
            "The filenames are the same, but their sizes differ: file_size {} != s3_object_len {}",
            &file_size,
            &s3_object_len
        );
        println!("⚠️ | There seems to be a file with the same filename already at the destination. They differ in sizes, I will assume that that they are different, so I will upload this one");
        Ok(false)
    }
}
