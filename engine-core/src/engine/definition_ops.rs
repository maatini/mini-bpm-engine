use std::sync::Arc;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::model::ProcessDefinition;

use super::WorkflowEngine;

impl WorkflowEngine {
    /// Returns a list of all deployed definitions (key, BPMN-ID, version, node count).
    pub async fn list_definitions(&self) -> Vec<(Uuid, String, i32, usize)> {
        self.definitions.list().await
    }

    /// Returns a given definition by key
    pub async fn get_definition(&self, key: &Uuid) -> Option<Arc<ProcessDefinition>> {
        self.definitions.get(key).await
    }

    /// Returns all versions of a specific BPMN process definition, sorted ascending.
    pub async fn list_definition_versions(&self, bpmn_id: &str) -> Vec<(Uuid, i32, usize)> {
        self.definitions
            .all_versions_of(bpmn_id)
            .await
            .into_iter()
            .map(|(key, def)| (key, def.version, def.nodes.len()))
            .collect()
    }

    /// Deploys a process definition so instances can be started from it.
    /// Deployment semantics: if a definition with the same BPMN process ID already
    /// exists, the new definition receives a fresh UUID key and an incremented version.
    /// Existing running instances continue on their original definition untouched.
    /// Returns (definition_key, version).
    pub async fn deploy_definition(&self, definition: ProcessDefinition) -> (Uuid, i32) {
        // Find highest version of existing definitions with matching ID
        let highest_version = self.definitions.highest_version(&definition.id).await;

        let key = definition.key; // Always use a unique key
        let version = highest_version.map(|v| v + 1).unwrap_or(definition.version);

        let mut def = definition;
        def.key = key;
        def.version = version;

        let sub_processes = std::mem::take(&mut def.sub_processes);
        for mut sub in sub_processes {
            // Assign a sub-process ID if needed, although it already has one.
            sub.version = version;
            Box::pin(self.deploy_definition(sub)).await;
        }

        tracing::info!(
            "Deployed definition '{}' (v{}, key: {})",
            def.id,
            def.version,
            key
        );
        self.definitions.insert(key, Arc::new(def)).await;
        self.persist_definition(key).await;
        (key, version)
    }

    /// Deletes a process definition.
    /// If cascade is true, deletes all associated process instances first.
    pub async fn delete_definition(&self, definition_key: Uuid, cascade: bool) -> EngineResult<()> {
        if !self.definitions.contains_key(&definition_key).await {
            return Err(EngineError::NoSuchDefinition(definition_key));
        }

        // Check for instances
        let all_insts = self.instances.all().await;
        let mut associated_instances = Vec::new();
        for lk in all_insts.values() {
            if lk.read().await.definition_key == definition_key {
                associated_instances.push(lk.read().await.id);
            }
        }

        if !associated_instances.is_empty() {
            if !cascade {
                return Err(EngineError::DefinitionHasInstances(
                    associated_instances.len(),
                ));
            }
            // Cascade delete instances
            for instance_id in associated_instances {
                self.delete_instance(instance_id).await?;
            }
        }

        self.definitions.remove(&definition_key).await;

        if let Some(ref persistence) = self.persistence {
            persistence
                .delete_definition(&definition_key.to_string())
                .await?;
        }

        Ok(())
    }

    /// Deletes all process definition versions for a given BPMN ID.
    /// If cascade is true, deletes all associated process instances first.
    pub async fn delete_all_definitions(&self, bpmn_id: &str, cascade: bool) -> EngineResult<()> {
        let versions = self.definitions.all_versions_of(bpmn_id).await;
        for (key, _) in versions {
            self.delete_definition(key, cascade).await?;
        }
        Ok(())
    }
}
