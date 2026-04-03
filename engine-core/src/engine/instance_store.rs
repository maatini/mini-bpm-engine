use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use crate::ProcessInstance;

/// Thread-safe registry for managing process instances.
/// 
/// Allows retrieving a read/write lock for an individual instance
/// without locking the entire engine.
#[derive(Clone, Default)]
pub struct InstanceStore {
    inner: Arc<RwLock<HashMap<Uuid, Arc<RwLock<ProcessInstance>>>>>,
}

impl InstanceStore {
    /// Creates a new instance store
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Inserts a newly started process instance, wrapped in a lock.
    pub async fn insert(&self, key: Uuid, instance: ProcessInstance) {
        self.inner.write().await.insert(key, Arc::new(RwLock::new(instance)));
    }

    /// Retrieves the lock for a specific process instance
    pub async fn get(&self, key: &Uuid) -> Option<Arc<RwLock<ProcessInstance>>> {
        self.inner.read().await.get(key).cloned()
    }

    /// Deletes an instance from the store
    pub async fn remove(&self, key: &Uuid) -> Option<Arc<RwLock<ProcessInstance>>> {
        self.inner.write().await.remove(key)
    }

    /// Calculates the number of instances present in the store
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }
    
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }

    /// Clears the entire store (mostly for tests)
    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }

    /// Get all instances (clones the list of locks)
    pub async fn all(&self) -> HashMap<Uuid, Arc<RwLock<ProcessInstance>>> {
        self.inner.read().await.clone()
    }
}
