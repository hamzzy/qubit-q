use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::error::ModelManagerError;
use crate::metadata::{ModelId, ModelMetadata};

/// Trait for model registry operations.
#[async_trait]
pub trait ModelRegistry: Send + Sync {
    async fn register(&self, metadata: ModelMetadata) -> Result<(), ModelManagerError>;
    async fn get(&self, id: &ModelId) -> Result<Option<ModelMetadata>, ModelManagerError>;
    async fn list_all(&self) -> Result<Vec<ModelMetadata>, ModelManagerError>;
    async fn remove(&self, id: &ModelId) -> Result<(), ModelManagerError>;
    async fn update_last_used(&self, id: &ModelId) -> Result<(), ModelManagerError>;
    async fn lru_candidate(&self) -> Result<Option<ModelId>, ModelManagerError>;
}

/// In-memory registry backed by a JSON file on disk.
pub struct InMemoryRegistry {
    models: Arc<RwLock<HashMap<ModelId, ModelMetadata>>>,
    registry_path: PathBuf,
}

impl InMemoryRegistry {
    /// Create a new registry, loading from `registry.json` in `models_dir` if it exists.
    pub fn new(models_dir: &std::path::Path) -> Result<Self, ModelManagerError> {
        let registry_path = models_dir.join("registry.json");
        let models = if registry_path.exists() {
            let data = std::fs::read_to_string(&registry_path)?;
            let list: Vec<ModelMetadata> = serde_json::from_str(&data)?;
            let map: HashMap<ModelId, ModelMetadata> =
                list.into_iter().map(|m| (m.id.clone(), m)).collect();
            info!(count = map.len(), "Loaded registry from disk");
            map
        } else {
            HashMap::new()
        };

        Ok(Self {
            models: Arc::new(RwLock::new(models)),
            registry_path,
        })
    }

    /// Persist current registry state to disk.
    async fn persist(&self) -> Result<(), ModelManagerError> {
        let models = self.models.read().await;
        let list: Vec<&ModelMetadata> = models.values().collect();
        let json = serde_json::to_string_pretty(&list)?;

        if let Some(parent) = self.registry_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.registry_path, json).await?;
        Ok(())
    }
}

#[async_trait]
impl ModelRegistry for InMemoryRegistry {
    async fn register(&self, metadata: ModelMetadata) -> Result<(), ModelManagerError> {
        let id = metadata.id.clone();
        {
            let mut models = self.models.write().await;
            models.insert(id.clone(), metadata);
        }
        self.persist().await?;
        info!(%id, "Model registered");
        Ok(())
    }

    async fn get(&self, id: &ModelId) -> Result<Option<ModelMetadata>, ModelManagerError> {
        let models = self.models.read().await;
        Ok(models.get(id).cloned())
    }

    async fn list_all(&self) -> Result<Vec<ModelMetadata>, ModelManagerError> {
        let models = self.models.read().await;
        Ok(models.values().cloned().collect())
    }

    async fn remove(&self, id: &ModelId) -> Result<(), ModelManagerError> {
        {
            let mut models = self.models.write().await;
            if models.remove(id).is_none() {
                warn!(%id, "Attempted to remove non-existent model");
                return Err(ModelManagerError::NotFound(id.clone()));
            }
        }
        self.persist().await?;
        info!(%id, "Model removed");
        Ok(())
    }

    async fn update_last_used(&self, id: &ModelId) -> Result<(), ModelManagerError> {
        {
            let mut models = self.models.write().await;
            let model = models
                .get_mut(id)
                .ok_or_else(|| ModelManagerError::NotFound(id.clone()))?;
            model.last_used = Utc::now();
        }
        self.persist().await?;
        Ok(())
    }

    async fn lru_candidate(&self) -> Result<Option<ModelId>, ModelManagerError> {
        let models = self.models.read().await;
        let candidate = models
            .values()
            .min_by_key(|m| m.last_used)
            .map(|m| m.id.clone());
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::dummy_metadata;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_register_and_get() {
        let dir = TempDir::new().unwrap();
        let registry = InMemoryRegistry::new(&dir.path().to_path_buf()).unwrap();

        let meta = dummy_metadata();
        registry.register(meta.clone()).await.unwrap();

        let result = registry.get(&meta.id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Test Model");
    }

    #[tokio::test]
    async fn test_list_and_remove() {
        let dir = TempDir::new().unwrap();
        let registry = InMemoryRegistry::new(&dir.path().to_path_buf()).unwrap();

        let meta = dummy_metadata();
        registry.register(meta.clone()).await.unwrap();

        let list = registry.list_all().await.unwrap();
        assert_eq!(list.len(), 1);

        registry.remove(&meta.id).await.unwrap();
        let list = registry.list_all().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        {
            let registry = InMemoryRegistry::new(&path).unwrap();
            registry.register(dummy_metadata()).await.unwrap();
        }

        // Reload from disk
        let registry = InMemoryRegistry::new(&path).unwrap();
        let list = registry.list_all().await.unwrap();
        assert_eq!(list.len(), 1);
    }
}
