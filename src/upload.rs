use aws_sdk_s3::{
    operation::create_multipart_upload::CreateMultipartUploadOutput,
    primitives::SdkBody,
    types::{CompletedMultipartUpload, CompletedPart},
    Client,
};
use aws_smithy_runtime_api::http::Request;
use aws_smithy_types::byte_stream::{ByteStream, Length};
use std::{
    convert::Infallible,
    fmt::Write,
    fs::File,
    io::prelude::*,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body::{Body, SizeHint};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};

use crate::file_management;
use crate::utils;

// NOTE: Anything smaller than 5MB causes the uploads to be slow(er)
// The PART_SIZE needs to be at least 5MB
const PART_SIZE: u64 = 5 * 1024 * 1024; // 5MB

// NOTE: The upload with progress is from this example:
// https://github.com/awsdocs/aws-doc-sdk-examples/blob/main/rustv1/examples/s3/src/bin/put-object-progress.rs
// It currently (2024-03-11) relies on older versions of `http` and `http-body` crates (0.x.x)
// I have not managed to get it working with the latest ones due to the `SdkBody` not implementing
// `Body` from these latest versions.
// TODO: Speak to the AWS Rust SDK team to get this working
struct ProgressTracker {
    bytes_written: u64,
    content_length: u64,
    bar: ProgressBar,
}

impl ProgressTracker {
    fn track(&mut self, len: u64) {
        self.bytes_written += len;
        let progress = self.bytes_written as f64 / self.content_length as f64;
        log::info!("Read {} bytes, progress: {:.2}&", len, progress * 100.0);
        if self.content_length != self.bytes_written {
            self.bar.set_position(self.bytes_written);
        } else {
            self.bar.finish_with_message("GOODBYE UPLOAD");
        }
    }
}

// NOTE: I have no idea what Pin projection is
// TODO: Learn what Pin projection is
#[pin_project::pin_project]
pub struct ProgressBody<InnerBody> {
    #[pin]
    inner: InnerBody,
    progress_tracker: ProgressTracker,
}

impl ProgressBody<SdkBody> {
    pub fn replace(value: Request<SdkBody>) -> Result<Request<SdkBody>, Infallible> {
        let value = value.map(|body| {
            let len = body.content_length().expect("upload body sized"); // TODO:panics
            let body = ProgressBody::new(body, len);
            SdkBody::from_body_0_4(body)
        });
        Ok(value)
    }
}

// inner body is an implementation of `Body` as far as i understand
impl<InnerBody> ProgressBody<InnerBody>
where
    InnerBody: Body<Data = Bytes, Error = aws_smithy_types::body::Error>,
{
    pub fn new(body: InnerBody, content_length: u64) -> Self {
        // creating the progress bar for uploads:
        let bar = ProgressBar::new(content_length);
        bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w,"{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

        Self {
            inner: body,
            progress_tracker: ProgressTracker {
                bytes_written: 0,
                content_length,
                bar,
            },
        }
    }
}

// Implementing `http_body::Body` for ProgressBody
impl<InnerBody> Body for ProgressBody<InnerBody>
where
    InnerBody: Body<Data = Bytes, Error = aws_smithy_types::body::Error>,
{
    type Data = Bytes;
    type Error = aws_smithy_types::body::Error;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let mut this = self.project();
        let result = match this.inner.as_mut().poll_data(cx) {
            //match this.inner.poll_data(cx) {
            Poll::Ready(Some(Ok(data))) => {
                this.progress_tracker.track(data.len() as u64);
                Poll::Ready(Some(Ok(data)))
            }
            Poll::Ready(None) => {
                // TODO: Figure out how to print something
                // at the end of the upload. Like a summary or whatever
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        };
        result
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        self.project().inner.poll_trailers(cx)
    }

    fn size_hint(&self) -> http_body::SizeHint {
        SizeHint::with_exact(self.progress_tracker.content_length)
    }
}

// FIX: Function has too many arguments
pub async fn upload_object(
    client: &Client,
    file_name: &PathBuf,
    key: &str,
    tags: file_management::ObjectTags,
    just_presign: bool,
    shuk_config: &utils::Config,
) -> Result<String, anyhow::Error> {
    // Getting file info so we can determine if we will do multi-part or not
    log::trace!(
        "Start of uploading {:?} to {}",
        &file_name,
        &shuk_config.bucket_name
    );

    log::trace!("Opening {:?}", &file_name);
    let mut file = match File::open(file_name) {
        Ok(file) => file,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to open file: {}", e));
        }
    };
    log::trace!("Getting {:?} metadata", &file_name);
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to get file metadata: {}", e));
        }
    };
    log::trace!("{:?} metadata: {:?}", &file_name, &metadata);
    let file_size = metadata.len();
    log::trace!("{:?} size: {:?}", &file_name, &file_size);

    let pref_key = format!(
        "{}{}",
        &shuk_config.bucket_prefix.clone().unwrap_or("".into()),
        key
    );
    log::trace!("Full Prefix key: {}", &pref_key);

    // We should only presign the file
    let presigned_url: String = if just_presign {
        log::trace!("The file needs to only be presigned.");

        let full_key = format!(
            "{}{}",
            shuk_config.bucket_prefix.clone().unwrap_or_default(),
            key,
        );

        let presigned_url = file_management::presign_file(
            client,
            &shuk_config.bucket_name,
            full_key.as_str(),
            //shuk_config.bucket_prefix.clone(),
            shuk_config.presigned_time,
        )
        .await?;
        println!("========================================");
        println!("📋 | Your file is already uploaded, re-pre-signing: ");
        println!("📋 | {}", presigned_url);

        presigned_url
    } else {
        log::trace!("The file needs to be uploaded.");
        // Actually upload the file
        // We need to do multi-part upload if file is larger than 4GB
        if file_size > 4294967296 {
            log::trace!(
                "The file is bigger than 4294967296. Size: {}. Using multi-part upload.",
                &file_size
            );

            println!("========================================");
            println!("💾 | File size is bigger than 4GB");
            println!("💾 | Using multi-part upload");
            println!(
                "🚀 | Uploading file: {}, to S3 Bucket: {} | 🚀",
                key, &shuk_config.bucket_name
            );
            println!("========================================");

            // A new bar is created here as we cannot use the same approach as with non multi-part
            // uploads
            let bar = ProgressBar::new(file_size);
            bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w,"{:.1}s", state.eta().as_secs_f64()).unwrap())
                .progress_chars("#>-"));

            let multipart_upload_res: CreateMultipartUploadOutput = client
                .create_multipart_upload()
                .bucket(&shuk_config.bucket_name)
                .key(&pref_key)
                .set_tagging(Some(tags.to_string()))
                .send()
                .await?;

            let upload_id = multipart_upload_res
                .upload_id()
                .ok_or_else(|| anyhow::anyhow!("Failed to get upload ID"))?;
            log::trace!(
                "Generated the upload_id for multi-part uploads: {}",
                &upload_id
            );

            let mut completed_parts = Vec::new();
            let mut part_number = 1;
            let mut file_position: u64 = 0;

            // main loop for file chunks
            log::trace!("Main loop for uploading file chunks starting...");
            while file_position < file_size {
                log::trace!("File Position: {}", &file_position);
                let bytes_remaining = file_size - file_position;
                log::trace!("Bytes Remaining: {}", &bytes_remaining);
                let part_size = std::cmp::min(bytes_remaining, PART_SIZE);
                log::trace!("Size of part: {}", &part_size);
                log::trace!("Part number : {}", &part_number);

                let mut part_data = vec![0; part_size as usize];
                if let Err(e) = file.read_exact(&mut part_data) {
                    return Err(anyhow::anyhow!("Failed to read file: {}", e));
                }

                let stream = match ByteStream::read_from()
                    .path(file_name)
                    .offset(file_position)
                    .length(Length::Exact(part_size))
                    .build()
                    .await {
                        Ok(bytestream) => bytestream,
                        Err(e) => {
                            // NOTE: If we cannot load the file into ByteStream, just error out
                            eprint!("Failed to load the file into ByteStream: {}", e);
                            std::process::exit(1);
                        }
                };

                bar.set_position(file_position);

                let upload_part_res = client
                    .upload_part()
                    .bucket(&shuk_config.bucket_name)
                    .key(&pref_key)
                    .upload_id(upload_id)
                    .part_number(part_number)
                    .body(stream)
                    .send()
                    .await?;

                let completed_part = CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(upload_part_res.e_tag.expect("Was unable to upload part"))
                    .build();

                completed_parts.push(completed_part);

                file_position += part_size;
                part_number += 1;
            }
            log::trace!("Completed chunk uploads");

            let completed_multipart_upload = CompletedMultipartUpload::builder()
                .set_parts(Some(completed_parts))
                .build();

            log::trace!("Sending complete_multipart_upload API call to S3 ");
            let _complete_multipart_upload_res = client
                .complete_multipart_upload()
                .bucket(&shuk_config.bucket_name)
                .key(&pref_key)
                .multipart_upload(completed_multipart_upload)
                .upload_id(upload_id)
                .send()
                .await?;
        } else {
            // There is no need for multi-part uploads, as the file is smaller than 4GB
            log::trace!(
                "The file is smaller than 4294967296. Size: {}. No need for multi-part upload.",
                &file_size
            );
            println!("========================================");
            println!(
                "🚀 | Uploading file: {}, to S3 Bucket: {} | 🚀",
                key, &shuk_config.bucket_name
            );
            println!("========================================");

            log::trace!("Reading file into body");
            let body = match ByteStream::read_from()
                .path(Path::new(file_name))
                .buffer_size(2048)
                .build()
                .await
            {
                Ok(stream) => stream,
                Err(e) => return Err(anyhow::anyhow!("Failed to create ByteStream: {}", e)),
            };

            log::trace!("Sending put_object API call to S3");
            let request = client
                .put_object()
                .bucket(&shuk_config.bucket_name)
                .key(&pref_key)
                .set_tagging(Some(tags.to_string()))
                .body(body);

            // for the progress bar
            let customized = request
                .customize()
                .map_request(ProgressBody::<SdkBody>::replace);
            let out = customized.send().await?;
            log::debug!("PutObjectOutput: {:?}", out);
        }

        // NOTE: Not sure if this should exist in this upload_object function
        // or do I move the logic away from here into the main.rs or some
        // wrapped function
        //
        // presign the file and return the URL
        let full_key = format!(
            "{}{}",
            shuk_config.bucket_prefix.clone().unwrap_or_default(),
            key,
        );
        let presigned_url = file_management::presign_file(
            client,
            &shuk_config.bucket_name,
            full_key.as_str(),
            //shuk_config.bucket_prefix.clone(),
            shuk_config.presigned_time,
        )
        .await?;
        println!("========================================");
        println!("📋 | Good job, here is your file: ");
        println!("📋 | {}", presigned_url);

        presigned_url
    };

    Ok(presigned_url)
}
