use std::{
    fmt::Write,
    convert::Infallible,
    pin::Pin,
    path::{Path,PathBuf}, 
    task::{Context,Poll},
};
use aws_sdk_s3::{
    primitives::{
        ByteStream,
        SdkBody,
    }, 
    Client
};
use aws_smithy_runtime_api::http::Request;

use bytes::Bytes;
use http_body::{Body, SizeHint};

use indicatif::{ProgressBar, ProgressState, ProgressStyle};

use tracing::{debug, info};

use crate::file_management;

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
   pub fn replace(value: Request<SdkBody> ) -> Result<Request<SdkBody>, Infallible>{
        let value =  value.map(|body| {
            let len = body.content_length().expect("upload body sized"); // TODO:panics
            let body = ProgressBody::new(body, len);
            SdkBody::from_body_0_4(body)
        });
        Ok(value)

    }

}

// inner body is an implementation of `Body` as far as i understand
impl<InnerBody> ProgressBody<InnerBody>
    where InnerBody: Body<Data = Bytes, Error = aws_smithy_types::body::Error>,
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
    where InnerBody: Body<Data = Bytes, Error = aws_smithy_types::body::Error>,
{
    type Data = Bytes;
    type Error = aws_smithy_types::body::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let mut this = self.project();
        let result = match this.inner.as_mut().poll_data(cx) {
        //match this.inner.poll_data(cx) {
            Poll::Ready(Some(Ok(data))) => {
                this.progress_tracker.track(data.len() as u64);
                Poll::Ready(Some(Ok(data)))
            },
            Poll::Ready(None) => {
                // TODO: Figure out how to print something
                // at the end of the upload. Like a summary or whatever
                debug!("done");
                Poll::Ready(None)
            },
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

pub async fn upload_object(client: &Client, bucket_name: &str, file_name: &PathBuf, key: &str, presigned_time: u64) -> Result<(), anyhow::Error>{
    // PRINTING FILE / BUCKET INFORMATION:
    println!("========================================");
    println!("🚀 | Uploading file: {}, to S3 Bucket: {} | 🚀", key, bucket_name);
    println!("========================================");

    let body = ByteStream::read_from()
        .path(Path::new(file_name))
        .buffer_size(2048)
        .build()
        .await?;

    let request = client.put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body);
    
    // for the progress bar
    let customized = request.customize().map_request(ProgressBody::<SdkBody>::replace);
    let out = customized.send().await?;
    debug!("PutObjectOutput: {:?}", out);

    // NOTE: Not sure if this should exist in this upload_object function
    // or do I move the logic away from here into the main.rs or some
    // wrapped function
    //
    // presign the file and return the URL
    let presigned_url = file_management::presign_file(client, bucket_name, key, presigned_time).await?;
    println!("========================================");
    println!("📋 | Good job, here is your file: {}", presigned_url);

    Ok(())
}
