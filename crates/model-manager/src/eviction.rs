use std::path::PathBuf;

use crate::{ModelId, ModelManagerError, ModelRegistry};

/// Evict least-recently-used models until total registry storage is within quota.
///
/// `protected` models are never considered for eviction.
pub async fn evict_until_within_quota<R: ModelRegistry + ?Sized>(
    registry: &R,
    max_storage_bytes: u64,
    protected: &[ModelId],
) -> Result<Vec<ModelId>, ModelManagerError> {
    let mut evicted = Vec::new();

    loop {
        let current = registry.total_storage_bytes().await?;
        if current <= max_storage_bytes {
            return Ok(evicted);
        }

        let mut candidates = registry.list_all().await?;
        candidates.sort_by_key(|m| m.last_used);
        let candidate = candidates
            .into_iter()
            .find(|m| !protected.contains(&m.id))
            .ok_or(ModelManagerError::StorageQuotaExceeded {
                current,
                limit: max_storage_bytes,
            })?;

        let removed_id = candidate.id.clone();
        let removed_path = registry.remove_with_file(&candidate.id).await?;
        delete_model_file(&removed_path).await?;
        evicted.push(removed_id);
    }
}

async fn delete_model_file(path: &PathBuf) -> Result<(), ModelManagerError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ModelManagerError::EvictionFailed(format!(
            "failed to delete {}: {e}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryRegistry, ModelMetadata, QuantType};
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    fn metadata(
        id: &str,
        path: PathBuf,
        size_bytes: u64,
        last_used: chrono::DateTime<Utc>,
    ) -> ModelMetadata {
        ModelMetadata {
            id: id.into(),
            name: id.to_string(),
            path,
            quantization: QuantType::Q4KM,
            size_bytes,
            estimated_ram_bytes: size_bytes * 2,
            context_limit: 2048,
            sha256: String::new(),
            last_used,
            download_url: None,
            license: "unknown".into(),
            min_ram_bytes: 0,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn evicts_lru_first() {
        let dir = TempDir::new().unwrap();
        let registry = InMemoryRegistry::new(dir.path()).unwrap();

        let old_path = dir.path().join("old.gguf");
        let new_path = dir.path().join("new.gguf");
        tokio::fs::write(&old_path, vec![0; 10]).await.unwrap();
        tokio::fs::write(&new_path, vec![0; 10]).await.unwrap();

        registry
            .register(metadata(
                "old",
                old_path.clone(),
                10,
                Utc::now() - Duration::days(2),
            ))
            .await
            .unwrap();
        registry
            .register(metadata("new", new_path.clone(), 10, Utc::now()))
            .await
            .unwrap();

        let evicted = evict_until_within_quota(&registry, 10, &[]).await.unwrap();
        assert_eq!(evicted, vec![ModelId::from("old")]);
        assert!(!old_path.exists());
        assert!(new_path.exists());
    }

    #[tokio::test]
    async fn errors_when_only_protected_models_remain() {
        let dir = TempDir::new().unwrap();
        let registry = InMemoryRegistry::new(dir.path()).unwrap();

        let path = dir.path().join("protected.gguf");
        tokio::fs::write(&path, vec![0; 10]).await.unwrap();
        registry
            .register(metadata("protected", path, 10, Utc::now()))
            .await
            .unwrap();

        let err = evict_until_within_quota(&registry, 1, &[ModelId::from("protected")])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ModelManagerError::StorageQuotaExceeded { .. }
        ));
    }
}
