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

use tracing::{debug, info};

use crate::constants;
use crate::file_management;

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
        info!("Read {} bytes, progress: {:.2}&", len, progress * 100.0);
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
                debug!("done");
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

pub async fn upload_object(
    client: &Client,
    bucket_name: &str,
    file_name: &PathBuf,
    prefix: Option<String>,
    key: &str,
    presigned_time: u64,
) -> Result<String, anyhow::Error> {
    // Getting file info so we can determine if we will do multi-part or not
    let mut file = match File::open(file_name) {
        Ok(file) => file,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to open file: {}",e));
        }
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to get file metadata: {}",e));
        }
    };
    let file_size = metadata.len();

    let pref_key = format!("{}{}", prefix.clone().unwrap_or("".into()), key);

    // We need to do multi-part upload if file is larger than 4GB
    if file_size > 4294967296 {
        println!("========================================");
        println!("💾 | File size is bigger than 4GB");
        println!("💾 | Using multi-part upload");
        println!(
            "🚀 | Uploading file: {}, to S3 Bucket: {} | 🚀",
            key, bucket_name
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
            .bucket(bucket_name)
            .key(&pref_key)
            .tagging(constants::DEFAULT_OBJECT_TAG)
            .send()
            .await?;

         let upload_id = multipart_upload_res.upload_id()
             .ok_or_else(|| anyhow::anyhow!("Failed to get upload ID"))?;

        let mut completed_parts = Vec::new();
        let mut part_number = 1;
        let mut file_position: u64 = 0;

        // main loop for file chunks
        while file_position < file_size {
            let bytes_remaining = file_size - file_position;
            let part_size = std::cmp::min(bytes_remaining, PART_SIZE);

            let mut part_data = vec![0; part_size as usize];
            if let Err(e) = file.read_exact(&mut part_data) {
                return Err(anyhow::anyhow!("Failed to read file: {}", e));
            }

            let stream = ByteStream::read_from()
                .path(file_name)
                .offset(file_position)
                .length(Length::Exact(part_size))
                .build()
                .await
                .unwrap();

            bar.set_position(file_position);

            let upload_part_res = client
                .upload_part()
                .bucket(bucket_name)
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

        let completed_multipart_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        let _complete_multipart_upload_res = client
            .complete_multipart_upload()
            .bucket(bucket_name)
            .key(&pref_key)
            .multipart_upload(completed_multipart_upload)
            .upload_id(upload_id)
            .send()
            .await
            .unwrap();
    } else {
        // There is no need for multi-part uploads, as the file is smaller than 4GB
        println!("========================================");
        println!(
            "🚀 | Uploading file: {}, to S3 Bucket: {} | 🚀",
            key, bucket_name
        );
        println!("========================================");

        let body = match ByteStream::read_from()
            .path(Path::new(file_name))
            .buffer_size(2048)
            .build()
            .await
            {
                Ok(stream) => stream,
                Err(e) => return Err(anyhow::anyhow!("Failed to create ByteStream: {}",e)),
            };

        let request = client
            .put_object()
            .bucket(bucket_name)
            .key(&pref_key)
            .tagging(constants::DEFAULT_OBJECT_TAG)
            .body(body);

        // for the progress bar
        let customized = request
            .customize()
            .map_request(ProgressBody::<SdkBody>::replace);
        let out = customized.send().await?;
        debug!("PutObjectOutput: {:?}", out);
    }

    // NOTE: Not sure if this should exist in this upload_object function
    // or do I move the logic away from here into the main.rs or some
    // wrapped function
    //
    // presign the file and return the URL
    let presigned_url =
        file_management::presign_file(client, bucket_name, key, prefix, presigned_time).await?;
    println!("========================================");
    println!("📋 | Good job, here is your file: ");
    println!("📋 | {}", presigned_url);

    // if the clipboard is set to true, save to the system clipboard.
    // if not or not even set do not do that.
    // if clipboard.unwrap_or(false) {
    //     utils::set_into_clipboard(presigned_url);
    // }

    Ok(presigned_url)
}
