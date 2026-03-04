use std::path::Path;
use std::time::Duration;

use reqwest::header::RANGE;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tracing::warn;

use crate::ModelManagerError;

const DEFAULT_MAX_RETRIES: usize = 3;
const DEFAULT_BACKOFF_MS: u64 = 400;

/// Download execution summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadReport {
    pub resumed_from_bytes: u64,
    pub total_bytes: u64,
    pub retries: usize,
}

/// Incremental transfer progress for telemetry/UI updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferProgress {
    pub resumed_from_bytes: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub retries: usize,
}

/// Resume a local file copy into `destination`.
///
/// If `destination` already exists, copy resumes from its current size.
/// Returns `(resumed_from_bytes, total_destination_size)`.
pub async fn resume_copy_file(
    source: &Path,
    destination: &Path,
) -> Result<(u64, u64), ModelManagerError> {
    resume_copy_file_with_progress(source, destination, |_| {}).await
}

/// Resume a local file copy and emit progress callbacks.
pub async fn resume_copy_file_with_progress<F>(
    source: &Path,
    destination: &Path,
    on_progress: F,
) -> Result<(u64, u64), ModelManagerError>
where
    F: FnMut(TransferProgress) + Send,
{
    resume_copy_file_with_progress_and_cancel(source, destination, on_progress, || false).await
}

/// Resume a local file copy, emit progress callbacks, and support cancellation checks.
pub async fn resume_copy_file_with_progress_and_cancel<F, C>(
    source: &Path,
    destination: &Path,
    mut on_progress: F,
    should_cancel: C,
) -> Result<(u64, u64), ModelManagerError>
where
    F: FnMut(TransferProgress) + Send,
    C: Fn() -> bool + Send + Sync,
{
    if !source.exists() {
        return Err(ModelManagerError::FileNotFound(source.to_path_buf()));
    }

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let source_size = tokio::fs::metadata(source).await?.len();
    let resumed_from = tokio::fs::metadata(destination)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    if resumed_from > source_size {
        return Err(ModelManagerError::DownloadFailed(format!(
            "destination is larger than source (dest={}, source={})",
            resumed_from, source_size
        )));
    }

    on_progress(TransferProgress {
        resumed_from_bytes: resumed_from,
        downloaded_bytes: 0,
        total_bytes: Some(source_size),
        retries: 0,
    });

    let mut src = tokio::fs::OpenOptions::new()
        .read(true)
        .open(source)
        .await?;
    src.seek(std::io::SeekFrom::Start(resumed_from)).await?;

    let mut dst = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(destination)
        .await?;

    let mut buf = vec![0_u8; 64 * 1024];
    let mut downloaded_bytes = 0_u64;
    loop {
        if should_cancel() {
            return Err(ModelManagerError::DownloadCancelled);
        }
        let n = src.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await?;
        downloaded_bytes = downloaded_bytes.saturating_add(n as u64);
        on_progress(TransferProgress {
            resumed_from_bytes: resumed_from,
            downloaded_bytes,
            total_bytes: Some(source_size),
            retries: 0,
        });
    }
    dst.flush().await?;

    let final_size = tokio::fs::metadata(destination).await?.len();
    Ok((resumed_from, final_size))
}

/// Resume an HTTP(S) download into `destination`.
///
/// Returns `(resumed_from_bytes, total_destination_size)`.
pub async fn download_with_resume(
    url: &str,
    destination: &Path,
) -> Result<(u64, u64), ModelManagerError> {
    let report = download_with_resume_report(url, destination).await?;
    Ok((report.resumed_from_bytes, report.total_bytes))
}

/// Resume an HTTP(S) download with retry/backoff on transient failures.
pub async fn download_with_resume_report(
    url: &str,
    destination: &Path,
) -> Result<DownloadReport, ModelManagerError> {
    download_with_resume_report_and_progress(url, destination, |_| {}).await
}

/// Resume an HTTP(S) download with retry/backoff and progress callbacks.
pub async fn download_with_resume_report_and_progress<F>(
    url: &str,
    destination: &Path,
    on_progress: F,
) -> Result<DownloadReport, ModelManagerError>
where
    F: FnMut(TransferProgress) + Send,
{
    download_with_resume_report_and_progress_and_cancel(url, destination, on_progress, || false)
        .await
}

/// Resume an HTTP(S) download with retry/backoff, progress callbacks, and cancellation checks.
pub async fn download_with_resume_report_and_progress_and_cancel<F, C>(
    url: &str,
    destination: &Path,
    mut on_progress: F,
    should_cancel: C,
) -> Result<DownloadReport, ModelManagerError>
where
    F: FnMut(TransferProgress) + Send,
    C: Fn() -> bool + Send + Sync,
{
    let mut retries = 0usize;

    loop {
        if should_cancel() {
            return Err(ModelManagerError::DownloadCancelled);
        }

        match download_with_resume_once(url, destination, retries, &mut on_progress, &should_cancel)
            .await
        {
            Ok((resumed_from, total_size)) => {
                return Ok(DownloadReport {
                    resumed_from_bytes: resumed_from,
                    total_bytes: total_size,
                    retries,
                });
            }
            Err(e) if retries < DEFAULT_MAX_RETRIES => {
                retries += 1;
                let backoff_ms = DEFAULT_BACKOFF_MS * (1_u64 << (retries - 1));
                warn!(
                    url,
                    retries,
                    backoff_ms,
                    error = %e,
                    "Download attempt failed, retrying"
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn download_with_resume_once<F>(
    url: &str,
    destination: &Path,
    retries: usize,
    on_progress: &mut F,
    should_cancel: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(u64, u64), ModelManagerError>
where
    F: FnMut(TransferProgress) + Send,
{
    if should_cancel() {
        return Err(ModelManagerError::DownloadCancelled);
    }

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let existing_size = tokio::fs::metadata(destination)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let client = reqwest::Client::new();
    let mut request = client.get(url);
    if existing_size > 0 {
        request = request.header(RANGE, format!("bytes={existing_size}-"));
    }

    let mut response = request
        .send()
        .await
        .map_err(|e| ModelManagerError::DownloadFailed(e.to_string()))?;

    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let final_size = tokio::fs::metadata(destination)
            .await
            .map(|m| m.len())
            .unwrap_or(existing_size);
        return Ok((existing_size, final_size));
    }

    if !response.status().is_success() {
        return Err(ModelManagerError::DownloadFailed(format!(
            "download failed with status {}",
            response.status()
        )));
    }

    let is_resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT && existing_size > 0;
    let resumed_from = if is_resumed { existing_size } else { 0 };
    let total_bytes = response
        .content_length()
        .map(|len| len.saturating_add(resumed_from));

    let mut file = if is_resumed {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(destination)
            .await?
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(destination)
            .await?
    };

    let mut downloaded_bytes = 0_u64;
    on_progress(TransferProgress {
        resumed_from_bytes: resumed_from,
        downloaded_bytes,
        total_bytes,
        retries,
    });

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| ModelManagerError::DownloadFailed(e.to_string()))?
    {
        if should_cancel() {
            return Err(ModelManagerError::DownloadCancelled);
        }
        file.write_all(&chunk).await?;
        downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);
        on_progress(TransferProgress {
            resumed_from_bytes: resumed_from,
            downloaded_bytes,
            total_bytes,
            retries,
        });
    }
    file.flush().await?;

    let final_size = tokio::fs::metadata(destination).await?.len();
    Ok((resumed_from, final_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resumes_partial_copy() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        let dst = dir.path().join("dst.bin");

        let content = vec![7_u8; 4096];
        tokio::fs::write(&src, &content).await.unwrap();
        tokio::fs::write(&dst, &content[..1024]).await.unwrap();

        let (resumed_from, final_size) = resume_copy_file(&src, &dst).await.unwrap();
        assert_eq!(resumed_from, 1024);
        assert_eq!(final_size, 4096);

        let copied = tokio::fs::read(&dst).await.unwrap();
        assert_eq!(copied, content);
    }
}
