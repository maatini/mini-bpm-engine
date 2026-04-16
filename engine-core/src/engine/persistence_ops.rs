use super::WorkflowEngine;
use super::retry_queue::{INLINE_BACKOFF_MS, INLINE_RETRIES, PersistJob};
use crate::ProcessInstance;
use uuid::Uuid;

macro_rules! inline_retry {
    (
        $self:expr,                 // The WorkflowEngine context
        $call:expr,                 // The async future call (e.g. p.save_instance(&inst))
        $desc:expr,                 // Description for logging (e.g. format!("save_instance({})", id))
        $fallback_job:expr          // PersistJob for background retry
    ) => {
        let mut last_err = None;
        for attempt in 0..=INLINE_RETRIES {
            match $call.await {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) if attempt < INLINE_RETRIES => {
                    let delay = INLINE_BACKOFF_MS * 2u64.pow(attempt);
                    tracing::warn!(
                        "{} retry {}/{}: {} — backoff {}ms",
                        $desc,
                        attempt + 1,
                        INLINE_RETRIES,
                        e,
                        delay
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    last_err = Some(e);
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        if let Some(e) = last_err {
            $self.log_persistence_error(&$desc, &e);
            $self.enqueue_retry($fallback_job);
        }
    };
}

impl WorkflowEngine {
    /// Logs and counts a persistence error.
    pub(crate) fn log_persistence_error(&self, context: &str, err: impl std::fmt::Display) {
        self.persistence_error_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::error!("PERSISTENCE FAILURE [{}]: {}", context, err);
    }

    /// Enqueues a failed job for background retry (if retry queue is available).
    fn enqueue_retry(&self, job: PersistJob) {
        if let Some(ref tx) = self.retry_tx
            && let Err(e) = tx.send(job)
        {
            tracing::error!("Failed to enqueue retry job: {} (channel closed)", e);
        }
    }

    pub(crate) async fn record_history_event_from_snapshot(
        &self,
        instance_id: Uuid,
        event_type: crate::history::HistoryEventType,
        description: &str,
        actor_type: crate::history::ActorType,
        actor_id: Option<String>,
        old_snapshot: Option<&crate::history::DiffSnapshot>,
    ) {
        if let Some(p) = &self.persistence {
            let inst_arc = self.instances.get(&instance_id).await;

            let diff = match (old_snapshot, &inst_arc) {
                (Some(o), Some(lk)) => {
                    let inst = lk.read().await;
                    crate::history::calculate_diff_from_snapshot(o, &inst)
                }
                _ => crate::history::HistoryDiff {
                    variables: None,
                    status: None,
                    current_node: None,
                    human_readable: None,
                },
            };

            // Do not record if nothing changed for generic token move
            if diff.is_empty()
                && matches!(event_type, crate::history::HistoryEventType::TokenAdvanced)
            {
                return;
            }

            let mut entry = crate::history::HistoryEntry::new(
                instance_id,
                event_type,
                description,
                actor_type,
                actor_id,
            );
            if !diff.is_empty() {
                entry = entry.with_diff(diff);
            }
            if let Some(lk) = &inst_arc {
                let inst = lk.read().await;
                if let Some(def) = self.definitions.get(&inst.definition_key) {
                    entry.definition_version = Some(def.version);
                }
                entry = entry.with_node(inst.current_node.clone());

                // Snapshot heuristic: store a full snapshot every 8 audit log entries
                if !inst.audit_log.is_empty()
                    && inst.audit_log.len() % 8 == 0
                    && let Ok(json_state) = serde_json::to_value(&*inst)
                {
                    entry = entry.with_snapshot(json_state);
                }
            }

            inline_retry!(
                self,
                p.append_history_entry(&entry),
                format!("record_history_event_from_snapshot({})", instance_id),
                PersistJob::AppendHistoryEntry(Box::new(entry))
            );
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
                    variables: None,
                    status: None,
                    current_node: None,
                    human_readable: None,
                },
            };

            // Do not record if nothing changed for generic token move
            if diff.is_empty()
                && matches!(event_type, crate::history::HistoryEventType::TokenAdvanced)
            {
                return;
            }

            // Vor dem Move von event_type: prüfen ob ein terminaler Event vorliegt
            let is_terminal = matches!(
                event_type,
                crate::history::HistoryEventType::InstanceCompleted
            );

            let mut entry = crate::history::HistoryEntry::new(
                instance_id,
                event_type,
                description,
                actor_type,
                actor_id,
            );
            if !diff.is_empty() {
                entry = entry.with_diff(diff);
            }
            if let Some(curr) = new_state.as_ref().or(old_state)
                && let Some(def) = self.definitions.get(&curr.definition_key)
            {
                entry.definition_version = Some(def.version);
            }

            if let Some(curr) = new_state {
                entry = entry.with_node(curr.current_node.clone());

                // Snapshot-Heuristik: alle 8 Audit-Log-Einträge ODER immer bei
                // Instanz-Abschluss — damit ist der letzte Zustand (Variablen,
                // current_node, completed_at) garantiert in der Historie enthalten.
                let periodic_snapshot = !curr.audit_log.is_empty()
                    && curr.audit_log.len() % 8 == 0;

                if (is_terminal || periodic_snapshot)
                    && let Ok(json_state) = serde_json::to_value(&curr)
                {
                    entry = entry.with_snapshot(json_state);
                }
            }

            inline_retry!(
                self,
                p.append_history_entry(&entry),
                format!("record_history_event({})", instance_id),
                PersistJob::AppendHistoryEntry(Box::new(entry))
            );
        }
    }

    /// Persists the current state of a process instance.
    pub(crate) async fn persist_instance(&self, instance_id: Uuid) {
        if let (Some(p), Some(inst_arc)) =
            (&self.persistence, self.instances.get(&instance_id).await)
        {
            let mut inst = inst_arc.write().await;
            if inst.audit_log.len() > crate::runtime::MAX_AUDIT_LOG_ENTRIES {
                let overflow = inst.audit_log.len() - crate::runtime::MAX_AUDIT_LOG_ENTRIES;
                inst.audit_log = inst.audit_log.split_off(overflow);
                inst.audit_log.insert(
                    0,
                    format!("... ({} older entries trimmed, see History API)", overflow),
                );
            }

            // Size guard: check serialized payload before NATS write
            let estimated_size = serde_json::to_vec(&*inst).map(|v| v.len()).unwrap_or(0);
            if estimated_size > crate::runtime::MAX_INSTANCE_PAYLOAD_BYTES {
                tracing::error!(
                    "Instance {} payload too large ({} bytes, limit {} bytes) — skipping persist",
                    instance_id,
                    estimated_size,
                    crate::runtime::MAX_INSTANCE_PAYLOAD_BYTES
                );
                self.log_persistence_error(
                    &format!("save_instance({})", instance_id),
                    format!(
                        "Payload size {} exceeds limit {}",
                        estimated_size,
                        crate::runtime::MAX_INSTANCE_PAYLOAD_BYTES
                    ),
                );
                return;
            }

            inline_retry!(
                self,
                p.save_instance(&inst),
                format!("save_instance({})", instance_id),
                PersistJob::SaveInstance(instance_id)
            );
        }
    }

    /// Persists a process definition to the KV store.
    pub(crate) async fn persist_definition(&self, key: Uuid) {
        if let (Some(p), Some(def)) = (&self.persistence, self.definitions.get(&key)) {
            inline_retry!(
                self,
                p.save_definition(&def),
                format!("save_definition({})", key),
                PersistJob::SaveDefinition(key)
            );
        }
    }

    /// Persists a pending user task to the KV store.
    pub(crate) async fn persist_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence
            && let Some(task_ref) = self.pending_user_tasks.get(&task_id)
        {
            inline_retry!(
                self,
                p.save_user_task(&task_ref),
                format!("save_user_task({})", task_id),
                PersistJob::SaveUserTask(task_id)
            );
        }
    }

    /// Deletes a completed pending user task from the KV store.
    pub(crate) async fn remove_persisted_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            inline_retry!(
                self,
                p.delete_user_task(task_id),
                format!("delete_user_task({})", task_id),
                PersistJob::DeleteUserTask(task_id)
            );
        }
    }

    /// Persists a pending service task to the KV store.
    pub(crate) async fn persist_service_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence
            && let Some(task_ref) = self.pending_service_tasks.get(&task_id)
        {
            inline_retry!(
                self,
                p.save_service_task(&task_ref),
                format!("save_service_task({})", task_id),
                PersistJob::SaveServiceTask(task_id)
            );
        }
    }

    /// Deletes a completed pending service task from the KV store.
    pub(crate) async fn remove_persisted_service_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            inline_retry!(
                self,
                p.delete_service_task(task_id),
                format!("delete_service_task({})", task_id),
                PersistJob::DeleteServiceTask(task_id)
            );
        }
    }

    /// Persists a pending timer to the KV store.
    pub(crate) async fn persist_timer(&self, timer_id: Uuid) {
        if let Some(p) = &self.persistence
            && let Some(timer_ref) = self.pending_timers.get(&timer_id)
        {
            inline_retry!(
                self,
                p.save_timer(&timer_ref),
                format!("save_timer({})", timer_id),
                PersistJob::SaveTimer(timer_id)
            );
        }
    }

    /// Deletes a completed or cancelled pending timer from the KV store.
    pub(crate) async fn remove_persisted_timer(&self, timer_id: Uuid) {
        if let Some(p) = &self.persistence {
            inline_retry!(
                self,
                p.delete_timer(timer_id),
                format!("delete_timer({})", timer_id),
                PersistJob::DeleteTimer(timer_id)
            );
        }
    }

    /// Persists a pending message catch event to the KV store.
    pub(crate) async fn persist_message_catch(&self, catch_id: Uuid) {
        if let Some(p) = &self.persistence
            && let Some(catch_ref) = self.pending_message_catches.get(&catch_id)
        {
            inline_retry!(
                self,
                p.save_message_catch(&catch_ref),
                format!("save_message_catch({})", catch_id),
                PersistJob::SaveMessageCatch(catch_id)
            );
        }
    }

    /// Deletes a completed or cancelled pending message catch event from the KV store.
    pub(crate) async fn remove_persisted_message_catch(&self, catch_id: Uuid) {
        if let Some(p) = &self.persistence {
            inline_retry!(
                self,
                p.delete_message_catch(catch_id),
                format!("delete_message_catch({})", catch_id),
                PersistJob::DeleteMessageCatch(catch_id)
            );
        }
    }

    /// Archives a completed instance to the history store (for history queries/filtering),
    /// but intentionally keeps the instance in the active DashMap and active persistence bucket.
    ///
    /// Completed instances remain visible in `list_instances()` with state `Completed`.
    /// Manual deletion via `DELETE /api/instances/{id}` is still possible.
    pub(crate) async fn archive_completed_instance(&self, instance_id: Uuid) {
        let Some(p) = &self.persistence else {
            return; // No persistence — instance stays in DashMap already
        };

        if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let inst = inst_arc.read().await;
            if let Err(e) = p.save_completed_instance(&inst).await {
                tracing::warn!("Failed to archive completed instance {instance_id} to history: {e}");
            }
        }
        // Deliberately NOT deleting from active persistence bucket or DashMap.
        // Completed instances stay available via list_instances() and GET /api/instances/{id}.
    }
}
