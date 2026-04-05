use crate::ProcessInstance;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Thread-safe registry for managing process instances.
///
/// Uses DashMap for lock-free concurrent access to individual instances.
/// Each instance is wrapped in its own RwLock for fine-grained locking.
#[derive(Clone, Default)]
pub struct InstanceStore {
    inner: Arc<DashMap<Uuid, Arc<RwLock<ProcessInstance>>>>,
}

impl InstanceStore {
    /// Creates a new instance store
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Inserts a newly started process instance, wrapped in a lock.
    pub async fn insert(&self, key: Uuid, instance: ProcessInstance) {
        self.inner.insert(key, Arc::new(RwLock::new(instance)));
    }

    /// Retrieves the lock for a specific process instance
    pub async fn get(&self, key: &Uuid) -> Option<Arc<RwLock<ProcessInstance>>> {
        self.inner.get(key).map(|r| r.value().clone())
    }

    /// Deletes an instance from the store
    pub async fn remove(&self, key: &Uuid) -> Option<Arc<RwLock<ProcessInstance>>> {
        self.inner.remove(key).map(|(_, v)| v)
    }

    /// Calculates the number of instances present in the store
    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.inner.len()
    }

    #[allow(dead_code)]
    pub async fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears the entire store (mostly for tests)
    #[allow(dead_code)]
    pub async fn clear(&self) {
        self.inner.clear();
    }

    /// Get all instances (clones the list of locks)
    pub async fn all(&self) -> HashMap<Uuid, Arc<RwLock<ProcessInstance>>> {
        self.inner
            .iter()
            .map(|r| (*r.key(), r.value().clone()))
            .collect()
    }
}
