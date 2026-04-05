use uuid::Uuid;
use crate::ProcessInstance;
use super::WorkflowEngine;
use super::retry_queue::{PersistJob, INLINE_RETRIES, INLINE_BACKOFF_MS};

impl WorkflowEngine {
    /// Logs and counts a persistence error.
    pub(crate) fn log_persistence_error(&self, context: &str, err: impl std::fmt::Display) {
        self.persistence_error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::error!("PERSISTENCE FAILURE [{}]: {}", context, err);
    }

    /// Enqueues a failed job for background retry (if retry queue is available).
    fn enqueue_retry(&self, job: PersistJob) {
        if let Some(ref tx) = self.retry_tx {
            if let Err(e) = tx.send(job) {
                tracing::error!("Failed to enqueue retry job: {} (channel closed)", e);
            }
        }
    }

    /// Helper to record a history entry for an instance, calculating the diff automatically.
    pub(crate) async fn record_history_event(
        &self,
        instance_id: Uuid,
        event_type: crate::history::HistoryEventType,
        description: &str,
        actor_type: crate::history::ActorType,
        actor_id: Option<String>,
        old_state: Option<&ProcessInstance>,
    ) {
        if let Some(p) = &self.persistence {
            let new_state = if let Some(lk) = self.instances.get(&instance_id).await {
                Some(lk.read().await.clone())
            } else {
                None
            };
            let diff = match (old_state, new_state.as_ref()) {
                (Some(o), Some(n)) => crate::history::calculate_diff(o, n),
                _ => crate::history::HistoryDiff {
                    variables: None, status: None, current_node: None, human_readable: None,
                },
            };

            // Do not record if nothing changed for generic token move
            if diff.is_empty()
                && matches!(event_type, crate::history::HistoryEventType::TokenAdvanced)
            {
                return;
            }

            let mut entry = crate::history::HistoryEntry::new(
                instance_id, event_type, description, actor_type, actor_id,
            );
            if !diff.is_empty() {
                entry = entry.with_diff(diff);
            }
            if let Some(curr) = new_state.as_ref().or(old_state) {
                if let Some(def) = self.definitions.get(&curr.definition_key).await {
                    entry.definition_version = Some(def.version);
                }
            }

            if let Some(curr) = new_state {
                entry = entry.with_node(curr.current_node.clone());

                // Snapshot heuristic: store a full snapshot every 8 audit log entries
                if !curr.audit_log.is_empty() && curr.audit_log.len() % 8 == 0 {
                    if let Ok(json_state) = serde_json::to_value(curr) {
                        entry = entry.with_snapshot(json_state);
                    }
                }
            }

            // Inline retry for history entries
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.append_history_entry(&entry).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tracing::warn!("History append retry {}/{}: {} — backoff {}ms",
                            attempt + 1, INLINE_RETRIES, e, delay);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(
                    &format!("record_history_event({})", instance_id), &e,
                );
                self.enqueue_retry(PersistJob::AppendHistoryEntry(Box::new(entry)));
            }
        }
    }

    /// Persists the current state of a process instance.
    /// Uses inline retry + background queue on failure.
    pub(crate) async fn persist_instance(&self, instance_id: Uuid) {
        if let (Some(p), Some(inst_arc)) = (&self.persistence, self.instances.get(&instance_id).await) {
            let mut inst = inst_arc.write().await;
            // Trim audit log to prevent NATS KV 1MB value overflow
            if inst.audit_log.len() > crate::engine::types::MAX_AUDIT_LOG_ENTRIES {
                let overflow = inst.audit_log.len() - crate::engine::types::MAX_AUDIT_LOG_ENTRIES;
                inst.audit_log = inst.audit_log.split_off(overflow);
                inst.audit_log.insert(
                    0,
                    format!("... ({} older entries trimmed, see History API)", overflow),
                );
            }

            // Inline retry
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.save_instance(&inst).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tracing::warn!("save_instance({}) retry {}/{}: {} — backoff {}ms",
                            instance_id, attempt + 1, INLINE_RETRIES, e, delay);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(&format!("save_instance({})", instance_id), &e);
                // Queue for background retry — worker will re-read from InstanceStore
                self.enqueue_retry(PersistJob::SaveInstance(instance_id));
            }
        }
    }

    /// Persists a process definition to the KV store.
    pub(crate) async fn persist_definition(&self, key: Uuid) {
        if let (Some(p), Some(def)) = (&self.persistence, self.definitions.get(&key).await) {
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.save_definition(&def).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tracing::warn!("save_definition({}) retry {}/{}: {} — backoff {}ms",
                            key, attempt + 1, INLINE_RETRIES, e, delay);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(&format!("save_definition({})", key), &e);
                self.enqueue_retry(PersistJob::SaveDefinition(key));
            }
        }
    }

    /// Persists a pending user task to the KV store.
    pub(crate) async fn persist_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(task_ref) = self.pending_user_tasks.get(&task_id) {
                let mut last_err = None;
                for attempt in 0..=INLINE_RETRIES {
                    match p.save_user_task(&*task_ref).await {
                        Ok(()) => { last_err = None; break; }
                        Err(e) if attempt < INLINE_RETRIES => {
                            let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                            tracing::warn!("save_user_task({}) retry {}/{}: {} — backoff {}ms",
                                task_id, attempt + 1, INLINE_RETRIES, e, delay);
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                            last_err = Some(e);
                        }
                        Err(e) => { last_err = Some(e); }
                    }
                }
                if let Some(e) = last_err {
                    self.log_persistence_error(&format!("save_user_task({})", task_id), &e);
                    self.enqueue_retry(PersistJob::SaveUserTask(task_id));
                }
            }
        }
    }

    /// Deletes a completed pending user task from the KV store.
    pub(crate) async fn remove_persisted_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.delete_user_task(task_id).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(&format!("delete_user_task({})", task_id), &e);
                self.enqueue_retry(PersistJob::DeleteUserTask(task_id));
            }
        }
    }

    /// Persists a pending service task to the KV store.
    pub(crate) async fn persist_service_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(task_ref) = self.pending_service_tasks.get(&task_id) {
                let mut last_err = None;
                for attempt in 0..=INLINE_RETRIES {
                    match p.save_service_task(&*task_ref).await {
                        Ok(()) => { last_err = None; break; }
                        Err(e) if attempt < INLINE_RETRIES => {
                            let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                            last_err = Some(e);
                        }
                        Err(e) => { last_err = Some(e); }
                    }
                }
                if let Some(e) = last_err {
                    self.log_persistence_error(&format!("save_service_task({})", task_id), &e);
                    self.enqueue_retry(PersistJob::SaveServiceTask(task_id));
                }
            }
        }
    }

    /// Deletes a completed pending service task from the KV store.
    pub(crate) async fn remove_persisted_service_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.delete_service_task(task_id).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(&format!("delete_service_task({})", task_id), &e);
                self.enqueue_retry(PersistJob::DeleteServiceTask(task_id));
            }
        }
    }

    /// Persists a pending timer to the KV store.
    pub(crate) async fn persist_timer(&self, timer_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(timer_ref) = self.pending_timers.get(&timer_id) {
                let mut last_err = None;
                for attempt in 0..=INLINE_RETRIES {
                    match p.save_timer(&*timer_ref).await {
                        Ok(()) => { last_err = None; break; }
                        Err(e) if attempt < INLINE_RETRIES => {
                            let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                            last_err = Some(e);
                        }
                        Err(e) => { last_err = Some(e); }
                    }
                }
                if let Some(e) = last_err {
                    self.log_persistence_error(&format!("save_timer({})", timer_id), &e);
                    self.enqueue_retry(PersistJob::SaveTimer(timer_id));
                }
            }
        }
    }

    /// Deletes a completed pending timer from the KV store.
    pub(crate) async fn remove_persisted_timer(&self, timer_id: Uuid) {
        if let Some(p) = &self.persistence {
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.delete_timer(timer_id).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(&format!("delete_timer({})", timer_id), &e);
                self.enqueue_retry(PersistJob::DeleteTimer(timer_id));
            }
        }
    }

    /// Persists a pending message catch to the KV store.
    pub(crate) async fn persist_message_catch(&self, catch_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(catch_ref) = self.pending_message_catches.get(&catch_id) {
                let mut last_err = None;
                for attempt in 0..=INLINE_RETRIES {
                    match p.save_message_catch(&*catch_ref).await {
                        Ok(()) => { last_err = None; break; }
                        Err(e) if attempt < INLINE_RETRIES => {
                            let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                            last_err = Some(e);
                        }
                        Err(e) => { last_err = Some(e); }
                    }
                }
                if let Some(e) = last_err {
                    self.log_persistence_error(&format!("save_message_catch({})", catch_id), &e);
                    self.enqueue_retry(PersistJob::SaveMessageCatch(catch_id));
                }
            }
        }
    }

    /// Deletes a completed pending message catch from the KV store.
    pub(crate) async fn remove_persisted_message_catch(&self, catch_id: Uuid) {
        if let Some(p) = &self.persistence {
            let mut last_err = None;
            for attempt in 0..=INLINE_RETRIES {
                match p.delete_message_catch(catch_id).await {
                    Ok(()) => { last_err = None; break; }
                    Err(e) if attempt < INLINE_RETRIES => {
                        let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    }
                    Err(e) => { last_err = Some(e); }
                }
            }
            if let Some(e) = last_err {
                self.log_persistence_error(&format!("delete_message_catch({})", catch_id), &e);
                self.enqueue_retry(PersistJob::DeleteMessageCatch(catch_id));
            }
        }
    }
}
