use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::info;

use crate::error::ModelManagerError;

/// Compute the SHA256 hash of a file, returning the hex-encoded digest.
pub async fn compute_sha256(path: &Path) -> Result<String, ModelManagerError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        let hash = hasher.finalize();
        Ok(format!("{hash:x}"))
    })
    .await
    .map_err(|e| ModelManagerError::Registry(e.to_string()))?
}

/// Verify that a file matches the expected SHA256 hash.
pub async fn verify_sha256(path: &Path, expected: &str) -> Result<(), ModelManagerError> {
    info!(path = %path.display(), "Verifying SHA256");
    let actual = compute_sha256(path).await?;
    if actual != expected {
        return Err(ModelManagerError::Sha256Mismatch {
            path: path.to_path_buf(),
            expected: expected.to_string(),
            actual,
        });
    }
    info!(path = %path.display(), "SHA256 verified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_compute_sha256() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let hash = compute_sha256(file.path()).await.unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[tokio::test]
    async fn test_verify_sha256_success() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let result = verify_sha256(
            file.path(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_verify_sha256_mismatch() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let result = verify_sha256(file.path(), "wrong_hash").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ModelManagerError::Sha256Mismatch { .. }
        ));
    }
}
