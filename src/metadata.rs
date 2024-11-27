use serde::{Deserialize, Serialize};
use aws_smithy_types::byte_stream::ByteStream;

use std::fs;
use std::time::Duration;
use std::path::Path;

use futures::stream::{self, StreamExt};

use chrono::{Utc, DateTime};

use indicatif::ProgressBar;

use dirs::home_dir;

use crate::constants;

use aws_smithy_types_convert::date_time::DateTimeExt;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub filename: String,
    pub start_hash: String,
    pub end_hash: String,
    pub file_size: i64,
    pub last_modified: DateTime<Utc>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ShukMetadata {
    pub files: Option<Vec<FileMetadata>>,
    pub updated_at: DateTime<Utc>,
}

impl ShukMetadata {
    pub fn get_most_recent_file(&self) -> Result<&FileMetadata, anyhow::Error> {
        self.files
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No files in metadata"))?
            .iter()
            .max_by_key(|file| &file.last_modified)
            .ok_or_else(|| anyhow::anyhow!("No files with valid dates found"))
    }
    pub fn file_count(&self) -> Result<usize, anyhow::Error> {
        self.files
            .as_ref()
            .map(|files| files.len())
            .ok_or_else(|| anyhow::anyhow!("No files in metadata"))
    }
    pub fn get_total_file_size(&self) -> Result<u64, anyhow::Error> {
        self.files
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No files in metadata"))?
            .iter()
            .map(|file| file.file_size as u64)
            .try_fold(0u64, |acc, size| {
                acc.checked_add(size)
                    .ok_or_else(||anyhow::anyhow!("Total size overflow"))
            })

    }
    pub fn get_total_file_size_formatted(&self) -> String {
        // Either get something or be 0
        let size = self.get_total_file_size().unwrap_or_default();
        Self::format_size(size)
    }

    fn format_size(size: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if size >= GB {
            format!("{:.2} GB", size as f64 / GB as f64 )
        } else if size >= MB {
            format!("{:.2} MB", size as f64 / MB as f64 )
        } else if size >= KB {
            format!("{:.2} KB", size as f64 / KB as f64 )
        } else {
            format!("{} B", size)

        }
    }
}

#[derive(Debug)]
pub struct MetadataTags {
    pub managed_by: String,
    pub do_not_scan: String,
}
// This converst the Struct into a list of tags the way the API accepts it
// NOTE: Maybe I can create a dedicated function for this instead of using Dispay
impl std::fmt::Display for MetadataTags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "managed_by={}&do_not_scan={}",
            self.managed_by, self.do_not_scan,
        )
    }
}

// Needed for the tagging
impl From<&MetadataTags> for String {
    fn from(tags: &MetadataTags) -> String {
        tags.to_string() // This uses your Display implementation
    }
}

async fn get_object_metadata(
    s3_client: &aws_sdk_s3::Client,
    bucket_name: &str,
    object: &aws_sdk_s3::types::Object,
) -> Result<Option<FileMetadata>, anyhow::Error> {
    log::debug!("Getting object metadata for: {:#?}", &object);
    // TODO: Handle unwarps
    let filename = object.key.clone().unwrap();
    log::debug!("Filename: {:#?}", &filename);
    let filesize = object.size.unwrap();
    log::debug!("File size: {:#?}", &filesize);
    let file_last_modified = object.last_modified
        .map_or(
            DateTime::from_timestamp_millis(0).unwrap(),
                |obj_date|
                obj_date.to_chrono_utc().unwrap()
        );
    log::debug!("File last modified: {:#?}", &file_last_modified);

    // NOTE: Specifically this this is the call that takes effort
    log::debug!("Getting object tags");
    let object_tags = s3_client
        .get_object_tagging()
        .bucket(bucket_name)
        .key(&filename)
        .send()
        .await;

    // Extracting the tags
    let tags = object_tags.unwrap();
    log::debug!("Tags: {:#?}", &tags);

    // if the file is managed by shuk
    let managed_by_tag = tags
        .tag_set()
        .iter()
        .find(|tag| tag.key() == "managed_by")
        .map(|tag| tag.value())
        .unwrap_or_default(); // Or just ""
                             
    log::debug!("Managed by Tag: {:#?}", &managed_by_tag);
    // ignore the files that need to be ignored (metadat files)
    let do_not_scan_tag = tags
        .tag_set()
        .iter()
        .find(|tag| tag.key() == "do_not_scan")
        .map(|tag| tag.value())
        .unwrap_or_default(); // Or just ""
    log::debug!("Do not scan by Tag: {:#?}", &do_not_scan_tag);

    if managed_by_tag == "shuk" && do_not_scan_tag != "true" {
        log::debug!("Do not scan by Tag: {:#?}", &do_not_scan_tag);
        // if it is - perform the metadata collection - otherwise skip
        let remote_start_hash = tags
            .tag_set()
            .iter()
            .find(|tag| tag.key() == "start_hash")
            .map(|tag| tag.value())
            .unwrap_or_default(); // Or just ""
        log::debug!("Remote Start hash Tag: {:#?}", &do_not_scan_tag);

        let remote_end_hash = tags
            .tag_set()
            .iter()
            .find(|tag| tag.key() == "end_hash")
            .map(|tag| tag.value())
            .unwrap_or_default(); // Or just ""
        log::debug!("Remote End hash Tag: {:#?}", &remote_end_hash);

        Ok(Some(FileMetadata {
            filename,
            file_size: filesize,
            start_hash: remote_start_hash.to_string(),
            end_hash: remote_end_hash.to_string(),
            last_modified: file_last_modified,
        }))
    } else {
        Ok(None)
    }
}

// This also updates metadata
pub async fn create_metadata(
    s3_client: &aws_sdk_s3::Client,
    bucket_name: &String,
    prefix: &String,
) -> Result<ShukMetadata, anyhow::Error> {
    log::debug!("Listing all objects");
    let objects = s3_client
        .list_objects_v2()
        .bucket(bucket_name)
        .prefix(prefix)
        .send()
        .await;

    let results = objects.unwrap();
    log::debug!("Objects: {:#?} ", &results);

    // START - Metadata spinner
    let updating_spinner = ProgressBar::new_spinner();
    updating_spinner.enable_steady_tick(Duration::from_millis(100));
    updating_spinner.set_message("Updating metadata ...");

    // Running parralel to speed up processing
    log::debug!("Starting parallel processing of objects");
    let concurrency_limit = 10;
    // Creating a stream from the iterator
    let files_metadata = stream::iter(results.contents())
        .map(|object| {
            // Mapping objects to Futures
            // For each object we are creaing a new async task
            let s3_client = s3_client.clone(); // Clone is cheap here
            async move {
                // async block that takes ownership of captured vars
                get_object_metadata(&s3_client, bucket_name, object).await
            }
            // Creates a stream of futres - each ne being a single object processing task
        })
        // This is the magic of parallel processing
        // this will process up to thelimit se set at the same time
        // as each future completes new one is started
        // The results come back in the order they complete - not necessarily in the original order
        // which is absolutely fine for us
        .buffer_unordered(concurrency_limit)
        // Here we filtar and process at the same time.
        // Processes each results and keeps the successi with Some.
        // Discards any errors or None results
        // It's async move as we are dealing wiht futures
        .filter_map(|result| async move {
            match result {
                Ok(Some(metadata)) => Some(metadata),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .await;
    log::debug!("Finished processing objects: {:#?}", &files_metadata);

    let metadata = ShukMetadata {
        files: Some(files_metadata),
        updated_at: Utc::now(),
    };
    log::debug!("Metadata: {:#?}", &metadata);
    // END - Metadata spinner
    updating_spinner.finish_with_message("✅ Done updating metadata");
    println!("----------------------------------------");

    Ok(metadata)
}

pub async fn save_metadata(
    s3_client: &aws_sdk_s3::Client,
    bucket_name: &String,
    prefix: &String,
    metadata: &ShukMetadata,
    tags: &MetadataTags,
) -> Result<(), anyhow::Error> {
    log::debug!("Saving metadata");
    let filename = constants::METADATA_FILE_NAME;
    let home_dir = home_dir().expect("Failed to get HOME directory");
    let config_dir = home_dir.join(format!(".config/{}", constants::CONFIG_DIR_NAME));
    let local_metadata_path = config_dir.join(constants::METADATA_FILE_NAME);

    // Saving locally
    log::debug!("Saving file locally");
    fs::write(&local_metadata_path,serde_json::to_string_pretty(&metadata)?)?;

    log::debug!("Reading file into body");
    let body = match ByteStream::read_from()
        .path(Path::new(&local_metadata_path))
        .buffer_size(2048)
        .build()
        .await
    {
        Ok(stream) => stream,
        Err(e) => return Err(anyhow::anyhow!("Failed to create ByteStream: {}", e)),
    };

    // Save the file to the S3 bucket
    log::debug!("Uploading metadata to the S3 bucket");
    match s3_client
        .put_object()
        .bucket(bucket_name)
        .body(body)
        .key(format!("{}{}", prefix, filename))
        .set_tagging(Some(tags.to_string()))
        .send()
        .await
    {
        Ok(_) => {
            log::debug!("Upload has been a success");
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!(
            "Unable to upload metadata to the S3 Location: {}",
            e
        )),
    }
}
