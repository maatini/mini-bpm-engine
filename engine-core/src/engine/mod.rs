use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::model::{BpmnElement, ProcessDefinition, Token, FileReference};
use crate::persistence::WorkflowPersistence;
pub mod types;
pub(crate) mod instance_store;
pub(crate) mod registry;
pub(crate) mod executor;
pub(crate) mod gateway;
pub(crate) mod boundary;
mod service_task;

pub use types::*;

/// The central workflow engine managing definitions, instances, and handlers.
pub struct WorkflowEngine {
    pub(crate) definitions: registry::DefinitionRegistry,
    pub(crate) instances: crate::engine::instance_store::InstanceStore,
    pub(crate) pending_user_tasks: Vec<PendingUserTask>,
    pub(crate) pending_service_tasks: Vec<PendingServiceTask>,
    pub(crate) pending_timers: Vec<PendingTimer>,
    pub(crate) pending_message_catches: Vec<PendingMessageCatch>,
    pub(crate) persistence: Option<Arc<dyn WorkflowPersistence>>,
    pub(crate) script_engine: rhai::Engine,
}

impl WorkflowEngine {
    /// Creates a new, empty engine.
    pub fn new() -> Self {
        log::info!("WorkflowEngine initialized");
        Self {
            definitions: registry::DefinitionRegistry::new(),
            instances: crate::engine::instance_store::InstanceStore::new(),
            pending_user_tasks: Vec::new(),
            pending_service_tasks: Vec::new(),
            pending_timers: Vec::new(),
            pending_message_catches: Vec::new(),
            persistence: None,
            script_engine: rhai::Engine::new(),
        }
    }

    /// Creates a new engine equipped with the InMemoryPersistence backend.
    pub fn with_in_memory_persistence() -> Self {
        let p = Arc::new(crate::persistence_in_memory::InMemoryPersistence::new());
        Self::new().with_persistence(p)
    }

    /// Attaches a persistence layer to the engine.
    pub fn with_persistence(mut self, persistence: Arc<dyn WorkflowPersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Sets the persistence layer (builder-style alternative to `with_persistence`).
    pub fn set_persistence(&mut self, persistence: Arc<dyn WorkflowPersistence>) {
        self.persistence = Some(persistence);
    }

    /// Restores a process instance from persistence (e.g. on server startup).
    pub async fn restore_instance(&mut self, instance: ProcessInstance) {
        log::info!("Restored instance {} (def: {})", instance.id, instance.definition_key);
        self.instances.insert(instance.id, instance).await;
    }

    /// Restores a pending user task from persistence.
    pub fn restore_user_task(&mut self, task: PendingUserTask) {
        log::info!("Restored user task {} (instance: {})", task.task_id, task.instance_id);
        self.pending_user_tasks.push(task);
    }

    /// Restores a pending service task from persistence.
    pub fn restore_service_task(&mut self, task: PendingServiceTask) {
        log::info!("Restored service task {} (instance: {})", task.id, task.instance_id);
        self.pending_service_tasks.push(task);
    }

    /// Returns summary statistics for monitoring dashboards.
    pub async fn get_stats(&self) -> EngineStats {
        let all_insts = self.instances.all().await;
        let mut running = 0; let mut comp = 0; let mut w_user = 0; let mut w_serv = 0;
        for lk in all_insts.values() {
            let st = &lk.read().await.state;
            match st {
                InstanceState::Running => running += 1,
                InstanceState::Completed => comp += 1,
                InstanceState::WaitingOnUserTask{..} => w_user += 1,
                InstanceState::WaitingOnServiceTask{..} => w_serv += 1,
                _ => {}
            }
        }
        EngineStats {
            definitions_count: self.definitions.len().await,
            instances_total: all_insts.len(),
            instances_running: running,
            instances_completed: comp,
            instances_waiting_user: w_user,
            instances_waiting_service: w_serv,
            pending_user_tasks: self.pending_user_tasks.len(),
            pending_service_tasks: self.pending_service_tasks.len(),
        }
    }

    /// Returns a list of all deployed definitions (key, BPMN-ID, node count).
    pub async fn list_definitions(&self) -> Vec<(Uuid, String, usize)> {
        self.definitions.list().await
    }

    // ----- History Recording -----------------------------------------------

    /// Helper to record a history entry for an instance, calculating the diff automatically.
    async fn record_history_event(
        &self,
        instance_id: Uuid,
        event_type: crate::history::HistoryEventType,
        description: &str,
        actor_type: crate::history::ActorType,
        actor_id: Option<String>,
        old_state: Option<&ProcessInstance>,
    ) {
        if let Some(p) = &self.persistence {
            let new_state = if let Some(lk) = self.instances.get(&instance_id).await { Some(lk.read().await.clone()) } else { None };
            let diff = match (old_state, new_state.as_ref()) {
                (Some(o), Some(n)) => crate::history::calculate_diff(o, n),
                _ => crate::history::HistoryDiff { 
                    variables: None, status: None, current_node: None, human_readable: None 
                },
            };
            
            // Do not record if nothing changed for generic token move
            if diff.is_empty() && matches!(event_type, crate::history::HistoryEventType::TokenAdvanced) {
                return;
            }

            let mut entry = crate::history::HistoryEntry::new(
                instance_id, event_type, description, actor_type, actor_id);
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

                // Snapshot-Heuristik: Alle 8 Audit-Log Einträge einen Snapshot speichern
                if !curr.audit_log.is_empty() && curr.audit_log.len() % 8 == 0 {
                    if let Ok(json_state) = serde_json::to_value(curr) {
                        entry = entry.with_snapshot(json_state);
                    }
                }
            }

            if let Err(e) = p.append_history_entry(&entry).await {
                log::error!("Failed to record history entry for {}: {}", instance_id, e);
            }
        }
    }

    /// Persists the current state of a process instance (if a persistence
    /// layer is configured). Logs and swallows errors.
    async fn persist_instance(&self, instance_id: Uuid) {
        if let (Some(p), Some(inst_arc)) = (&self.persistence, self.instances.get(&instance_id).await) {
            let inst = inst_arc.read().await;
            if let Err(e) = p.save_instance(&inst).await {
                log::error!("Failed to persist instance {}: {}", instance_id, e);
            }
        }
    }

    /// Persists a process definition to the KV store.
    async fn persist_definition(&self, key: Uuid) {
        if let (Some(p), Some(def)) = (&self.persistence, self.definitions.get(&key).await) {
            if let Err(e) = p.save_definition(&def).await {
                log::error!("Failed to persist definition {}: {}", key, e);
            }
        }
    }

    /// Persists a pending user task to the KV store.
    async fn persist_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(task) = self.pending_user_tasks.iter().find(|t| t.task_id == task_id) {
                if let Err(e) = p.save_user_task(task).await {
                    log::error!("Failed to persist user task {}: {}", task_id, e);
                }
            }
        }
    }

    /// Deletes a completed pending user task from the KV store.
    async fn remove_persisted_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Err(e) = p.delete_user_task(task_id).await {
                log::error!("Failed to delete persisted user task {}: {}", task_id, e);
            }
        }
    }

    /// Persists a pending service task to the KV store.
    pub(crate) async fn persist_service_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(task) = self.pending_service_tasks.iter().find(|t| t.id == task_id) {
                if let Err(e) = p.save_service_task(task).await {
                    log::error!("Failed to persist external task {}: {}", task_id, e);
                }
            }
        }
    }

    /// Deletes a completed pending service task from the KV store.
    pub(crate) async fn remove_persisted_service_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Err(e) = p.delete_service_task(task_id).await {
                log::error!("Failed to delete persisted external task {}: {}", task_id, e);
            }
        }
    }

    /// Persists a pending timer to the KV store.
    async fn persist_timer(&self, timer_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(timer) = self.pending_timers.iter().find(|t| t.id == timer_id) {
                if let Err(e) = p.save_timer(timer).await {
                    log::error!("Failed to persist timer {}: {}", timer_id, e);
                }
            }
        }
    }

    /// Deletes a completed pending timer from the KV store.
    async fn remove_persisted_timer(&self, timer_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Err(e) = p.delete_timer(timer_id).await {
                log::error!("Failed to delete persisted timer {}: {}", timer_id, e);
            }
        }
    }

    /// Persists a pending message catch to the KV store.
    async fn persist_message_catch(&self, catch_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(catch) = self.pending_message_catches.iter().find(|t| t.id == catch_id) {
                if let Err(e) = p.save_message_catch(catch).await {
                    log::error!("Failed to persist message catch {}: {}", catch_id, e);
                }
            }
        }
    }

    /// Deletes a completed pending message catch from the KV store.
    async fn remove_persisted_message_catch(&self, catch_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Err(e) = p.delete_message_catch(catch_id).await {
                log::error!("Failed to delete persisted message catch {}: {}", catch_id, e);
            }
        }
    }

    // ----- deployment ------------------------------------------------------

    /// Deploys a process definition so instances can be started from it.
    /// Deployment semantics: if a definition with the same BPMN process ID already
    /// exists, the new definition receives a fresh UUID key and an incremented version.
    /// Existing running instances continue on their original definition untouched.
    /// Returns the new definition key (UUID).
    pub async fn deploy_definition(&mut self, definition: ProcessDefinition) -> Uuid {
        // Find highest version of existing definitions with matching ID
        let highest_version = self.definitions.highest_version(&definition.id).await;
            
        let key = definition.key; // Always use a unique key
        let version = highest_version.map(|v| v + 1).unwrap_or(definition.version);

        let mut def = definition;
        def.key = key;
        def.version = version;
        log::info!("Deployed definition '{}' (v{}, key: {})", def.id, def.version, key);
        self.definitions.insert(key, Arc::new(def)).await;
        self.persist_definition(key).await;
        key
    }

    // ----- handler registration --------------------------------------------
    // ----- starting instances ----------------------------------------------

    /// Starts a new process instance from a deployed definition.
    ///
    /// The definition must have a plain `StartEvent`.
    /// Delegates to `start_instance_with_variables` with an empty variable map.
    pub async fn start_instance(&mut self, definition_key: Uuid) -> EngineResult<Uuid> {
        self.start_instance_with_variables(definition_key, HashMap::new()).await
    }

    /// Starts a new process instance with pre-populated variables.
    ///
    /// Like `start_instance`, but the token carries initial variables from the
    /// caller. The instance's `variables` field is also seeded.
    pub async fn start_instance_with_variables(
        &mut self,
        definition_key: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        self.start_instance_with_variables_and_parent(definition_key, variables, None).await
    }

    /// Internal method to start an instance and link it to a parent call activity
    pub(crate) async fn start_instance_with_variables_and_parent(
        &mut self,
        definition_key: Uuid,
        mut variables: HashMap<String, Value>,
        parent_instance_id: Option<Uuid>,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        if matches!(start_element, BpmnElement::TimerStartEvent(_)) {
            return Err(EngineError::InvalidDefinition(
                "Use trigger_timer_start() for timer start events".into(),
            ));
        }

        let instance_id = Uuid::new_v4();
        let business_key = variables
            .remove("business_key")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let instance = ProcessInstance {
            id: instance_id,
            definition_key,
            business_key,
            parent_instance_id,
            state: InstanceState::Running,
            current_node: start_id.to_string(),
            audit_log: vec![format!(
                "▶ Process started at node '{start_id}' with {} variable(s)",
                variables.len()
            )],
            variables: variables.clone(),
            active_tokens: Vec::new(),
            join_barriers: std::collections::HashMap::new(),
        };

        log::info!(
            "Started instance {instance_id} of def key {definition_key} at node '{start_id}' with {} vars",
            variables.len()
        );

        self.instances.insert(instance_id, instance).await;
        
        // Record history for start
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceStarted,
            &format!("Started instance of process '{}'", def.id),
            crate::history::ActorType::Engine,
            None,
            None
        ).await;

        let token = Token::with_variables(start_id, variables);
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save initial token: {}", e);
            }
        }
        Box::pin(self.run_instance_batch(instance_id, token)).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }

    /// Spawns a call activity sub-process
    pub(crate) async fn spawn_call_activity(
        &mut self,
        child_def_key: Uuid,
        parent_instance_id: Uuid,
        called_node: String,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        let child_id = self.start_instance_with_variables_and_parent(child_def_key, variables, Some(parent_instance_id)).await?;
        
        self.record_history_event(
            parent_instance_id,
            crate::history::HistoryEventType::CallActivityStarted,
            &format!("Started Call Activity '{}' (child instance {})", called_node, child_id),
            crate::history::ActorType::Engine,
            None,
            None
        ).await;
        
        Ok(child_id)
    }

    /// Checks if a completed instance has a parent, and if so, resumes the parent.
    pub(crate) async fn resume_parent_if_needed(&mut self, completed_instance_id: Uuid) -> EngineResult<()> {
        let inst_arc = self.instances.get(&completed_instance_id).await.ok_or(EngineError::NoSuchInstance(completed_instance_id))?;
        let inst = inst_arc.read().await;
            
        let parent_id = match inst.parent_instance_id {
            Some(pid) => pid,
            None => return Ok(()),
        };
        
        let child_vars = inst.variables.clone();
        
        // Find the parent
        log::info!("Child instance {completed_instance_id} completed, resuming parent {parent_id}");
        
        let (called_node_id, token_to_resume, def_key) = {
            let parent_arc = self.instances.get(&parent_id).await.ok_or(EngineError::NoSuchInstance(parent_id))?;
            let mut parent = parent_arc.write().await;
                
            let (called_node_id, mut token_to_resume) = if let InstanceState::WaitingOnCallActivity { token, .. } = &parent.state {
                let t = token.clone();
                parent.state = InstanceState::Running;
                (parent.current_node.clone(), Some(t))
            } else {
                return Ok(());
            };
            
            parent.audit_log.push(format!("🔗 Call Activity '{called_node_id}' completed successfully"));
            
            let def_key = parent.definition_key;
            
            if let Some(active) = parent.active_tokens.iter_mut().find(|at| at.token.current_node == called_node_id && !at.completed) {
                active.token.variables.extend(child_vars.clone());
                token_to_resume = Some(active.token.clone());
            } else if let Some(ref mut linear_token) = token_to_resume {
                linear_token.variables.extend(child_vars.clone());
            }
            
            (called_node_id, token_to_resume, def_key)
        };
        
        self.record_history_event(
            parent_id,
            crate::history::HistoryEventType::CallActivityCompleted,
            &format!("Call Activity '{}' completed (child instance {})", called_node_id, completed_instance_id),
            crate::history::ActorType::Engine,
            None,
            None
        ).await;

        if let Some(mut token) = token_to_resume {
            let def = self.definitions.get(&def_key).await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
                
            self.run_end_scripts(parent_id, &mut token, &def, &called_node_id).await?;
                
            let next_node = crate::engine::executor::resolve_next_target(&def, &called_node_id, &token.variables)?;
            token.current_node = next_node.clone();
            
            if let Some(p_inst_arc) = self.instances.get(&parent_id).await {
            let mut p_inst = p_inst_arc.write().await;
                p_inst.current_node = next_node;
            }
            
            // Run the batch for the parent
            Box::pin(self.run_instance_batch(parent_id, token)).await?;
        }
        
        Ok(())
    }

    /// Simulates an external timer trigger that starts a timer-start-event process.
    ///
    /// Validates the duration against the definition, then spawns the instance.
    pub async fn trigger_timer_start(
        &mut self,
        definition_key: Uuid,
        provided_duration: Duration,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        match start_element {
            BpmnElement::TimerStartEvent(expected_dur) => {
                if *expected_dur != provided_duration {
                    return Err(EngineError::TimerMismatch {
                        expected: expected_dur.as_secs(),
                        provided: provided_duration.as_secs(),
                    });
                }
            }
            _ => {
                return Err(EngineError::InvalidDefinition(
                    "Start event is not a timer start event".into(),
                ));
            }
        }

        let start_id = start_id.to_string();
        let instance_id = Uuid::new_v4();
        let business_key = Uuid::new_v4().to_string();
        let instance = ProcessInstance {
            id: instance_id,
            definition_key,
            business_key,
            parent_instance_id: None,
            state: InstanceState::Running,
            current_node: start_id.clone(),
            audit_log: vec![format!(
                "⏰ Timer fired ({}s) — started at node '{start_id}'",
                provided_duration.as_secs()
            )],
            variables: HashMap::new(),
            active_tokens: Vec::new(),
            join_barriers: std::collections::HashMap::new(),
        };

        log::info!(
            "Timer-started instance {instance_id} of def key {definition_key} ({}s)",
            provided_duration.as_secs()
        );

        self.instances.insert(instance_id, instance).await;

        // Record history for start
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceStarted,
            &format!("Timer fired for instance of process '{}'", def.id),
            crate::history::ActorType::Timer,
            None,
            None
        ).await;

        let token = Token::new(&start_id);
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save initial token (timer): {}", e);
            }
        }
        self.run_instance_batch(instance_id, token).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }

    /// Schedules a timer that, after sleeping for the given duration,
    /// will trigger a timer-start instance. Returns immediately.
    ///
    /// Note: this uses `tokio::time::sleep` in a spawned task. The engine
    /// reference is not carried into the task — instead the caller should
    /// poll or use channels in a production setup. For the demo, we return
    /// the duration and let the main code handle it.
    pub async fn schedule_timer_start(
        &self,
        definition_key: Uuid,
        duration: Duration,
    ) -> EngineResult<()> {
        if !self.definitions.contains_key(&definition_key).await {
            return Err(EngineError::NoSuchDefinition(definition_key));
        }

        log::info!(
            "Scheduled timer for def key '{definition_key}' — will fire in {}s",
            duration.as_secs()
        );

        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            log::info!("⏰ Timer fired for def key '{definition_key}' after {}s", duration.as_secs());
            // In a real engine this would send a message via mpsc channel
            // to the engine to start the instance. For demo purposes we log.
        });

        Ok(())
    }



    // ----- Phase 1 API: timers and messages ---------------------------------

    pub async fn correlate_message(
        &mut self,
        message_name: String,
        business_key: Option<String>,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Vec<Uuid>> {
        let mut affected_instances = Vec::new();
        let mut to_resume = Vec::new();
        
        for catch in &self.pending_message_catches {
            if catch.message_name == message_name {
                if let Some(inst_arc) = self.instances.get(&catch.instance_id).await {
            let inst = inst_arc.read().await;
                    if let Some(ref bk) = business_key {
                        if &inst.business_key != bk {
                            continue;
                        }
                    }
                    to_resume.push(catch.id);
                    affected_instances.push(catch.instance_id);
                }
            }
        }
        
        for catch_id in to_resume {
            let idx = self.pending_message_catches.iter().position(|p| p.id == catch_id)
                .ok_or_else(|| EngineError::InvalidDefinition(format!("Message catch {catch_id} disappeared")))?;
            let catch = self.pending_message_catches.remove(idx);
            
            let mut token = catch.token;
            token.variables.extend(variables.clone());
            
            let old_state = if let Some(lk) = self.instances.get(&catch.instance_id).await { Some(lk.read().await.clone()) } else { None };
            let def_key = {
                let inst_arc = self.instances.get(&catch.instance_id).await.ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
        let mut inst = inst_arc.write().await;
                inst.state = InstanceState::Running;
                inst.audit_log.push(format!("✉️ Msg '{}' correlated, resuming '{catch_id}'", message_name));
                inst.definition_key
            };
            
            self.record_history_event(
                catch.instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                &format!("Message '{}' correlated", message_name),
                crate::history::ActorType::Engine,
                None,
                old_state.as_ref()
            ).await;
            
            let def = self.definitions.get(&def_key).await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
            let next = crate::engine::executor::resolve_next_target(&def, &catch.node_id, &token.variables)?;
            token.current_node = next.clone();
            
            {
                let inst_arc = self.instances.get(&catch.instance_id).await.ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
        let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }
            
            self.remove_persisted_message_catch(catch_id).await;
            self.run_instance_batch(catch.instance_id, token).await?;
        }
        
        let mut defs_to_start = Vec::new();
        let all_defs = self.definitions.all().await;
        for (def_key, def) in &all_defs {
            if let Some((_, BpmnElement::MessageStartEvent { message_name: ref_msg })) = def.start_event() {
                if ref_msg == &message_name {
                    defs_to_start.push(*def_key);
                }
            }
        }
        
        for def_key in defs_to_start {
            let new_id = self.start_instance_with_variables(def_key, variables.clone()).await?;
            if let Some(ref bk) = business_key {
                if let Some(inst_arc) = self.instances.get(&new_id).await {
            let mut inst = inst_arc.write().await;
                    inst.business_key = bk.clone();
                }
                self.persist_instance(new_id).await;
            }
            affected_instances.push(new_id);
        }
        
        Ok(affected_instances)
    }

    pub async fn process_timers(&mut self) -> EngineResult<usize> {
        let now = chrono::Utc::now();
        let mut expired = Vec::new();
        
        for timer in &self.pending_timers {
            if timer.expires_at <= now {
                expired.push(timer.id);
            }
        }
        
        let count = expired.len();
        for tid in expired {
            let idx = self.pending_timers.iter().position(|p| p.id == tid)
                .ok_or_else(|| EngineError::InvalidDefinition(format!("Timer {tid} disappeared")))?;
            let timer = self.pending_timers.remove(idx);
            
            let old_state = if let Some(lk) = self.instances.get(&timer.instance_id).await { Some(lk.read().await.clone()) } else { None };
            let def_key = {
                let inst_arc = self.instances.get(&timer.instance_id).await.ok_or(EngineError::NoSuchInstance(timer.instance_id))?;
        let mut inst = inst_arc.write().await;
                inst.state = InstanceState::Running;
                inst.audit_log.push(format!("⏱ Timer '{}' expired, resuming", timer.node_id));
                inst.definition_key
            };
            
            self.record_history_event(
                timer.instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                "Timer expired",
                crate::history::ActorType::Timer,
                None,
                old_state.as_ref()
            ).await;
            
            let mut token = timer.token;
            let def = self.definitions.get(&def_key).await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
            let next = crate::engine::executor::resolve_next_target(&def, &timer.node_id, &token.variables)?;
            token.current_node = next.clone();
            
            {
                let inst_arc = self.instances.get(&timer.instance_id).await.ok_or(EngineError::NoSuchInstance(timer.instance_id))?;
        let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }
            
            self.remove_persisted_timer(tid).await;
            self.run_instance_batch(timer.instance_id, token).await?;
        }
        
        Ok(count)
    }

    // ----- user task completion ---------------------------------------------

    /// Completes a pending user task by its task_id, optionally merging variables.
    ///
    /// Resumes the process instance after the user task.
    pub async fn complete_user_task(
        &mut self,
        task_id: Uuid,
        additional_vars: HashMap<String, Value>,
    ) -> EngineResult<()> {
        // Find and remove the pending task
        let idx = self
            .pending_user_tasks
            .iter()
            .position(|p| p.task_id == task_id)
            .ok_or_else(|| EngineError::TaskNotPending {
                task_id,
                actual_state: "not found in pending tasks".into(),
            })?;

        let pending = self.pending_user_tasks.remove(idx);
        let instance_id = pending.instance_id;

        // Merge additional variables into the token
        let mut token = pending.token;
        for (k, v) in additional_vars {
            token.variables.insert(k, v);
        }

        self.remove_persisted_user_task(task_id).await;
        self.cancel_boundary_timers(instance_id, &pending.node_id).await;

        let old_state = if let Some(lk) = self.instances.get(&instance_id).await { Some(lk.read().await.clone()) } else { None };

        log::info!(
            "Instance {instance_id}: completed user task '{}' (task_id: {task_id})",
            pending.node_id
        );

        let def_key = {
            let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.audit_log
                .push(format!("✅ User task '{}' completed", pending.node_id));
            
            if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                inst.state = InstanceState::Running;
            }
            inst.current_node = pending.node_id.clone();
            inst.definition_key
        };

        // Advance token to the next node
        let def = self
            .definitions
            .get(&def_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(def_key))?;
        // Current node's end scripts
        self.run_end_scripts(instance_id, &mut token, &def, &pending.node_id).await?;

        let next = crate::engine::executor::resolve_next_target(&def, &pending.node_id, &token.variables)?;

        token.current_node = next.clone();
        // Update instance current_node so UI highlights correctly
        let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        {
            let mut inst = inst_arc.write().await;
            inst.current_node = next;
        }
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save token after user task: {}", e);
            }
        }
        
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::TaskCompleted,
            &format!("User task '{}' completed", pending.node_id),
            crate::history::ActorType::User,
            Some(pending.assignee.clone()),
            old_state.as_ref()
        ).await;

        // Continue running
        self.run_instance_batch(instance_id, token).await
    }

    // ----- query helpers ---------------------------------------------------

    /// Returns the state of a process instance.
    pub async fn get_instance_state(&self, instance_id: Uuid) -> EngineResult<InstanceState> {
        if let Some(i_arc) = self.instances.get(&instance_id).await {
            Ok(i_arc.read().await.state.clone())
        } else {
            Err(EngineError::NoSuchInstance(instance_id))
        }
    }

    /// Returns the audit log of a process instance.
    pub async fn get_audit_log(&self, instance_id: Uuid) -> EngineResult<Vec<String>> {
        if let Some(i_arc) = self.instances.get(&instance_id).await {
            Ok(i_arc.read().await.audit_log.clone())
        } else {
            Err(EngineError::NoSuchInstance(instance_id))
        }
    }

    /// Returns all currently pending user tasks.
    pub fn get_pending_user_tasks(&self) -> &[PendingUserTask] {
        &self.pending_user_tasks
    }

    /// Returns all pending service tasks (for debugging / admin).
    pub fn get_pending_service_tasks(&self) -> &[PendingServiceTask] {
        &self.pending_service_tasks
    }

    /// Returns a list of all process instances (cloned).
    pub async fn list_instances(&self) -> Vec<ProcessInstance> {
        let all = self.instances.all().await;
        let mut out = Vec::with_capacity(all.len());
        for lk in all.values() {
            out.push(lk.read().await.clone());
        }
        out
    }

    /// Returns full details for a single process instance.
    pub async fn get_instance_details(&self, id: Uuid) -> EngineResult<ProcessInstance> {
        if let Some(i_arc) = self.instances.get(&id).await {
            Ok(i_arc.read().await.clone())
        } else {
            Err(EngineError::NoSuchInstance(id))
        }
    }

    /// Helper to cancel any pending boundary timers attached to a task node that is being completed/aborted.
    pub(crate) async fn cancel_boundary_timers(&mut self, instance_id: Uuid, task_node_id: &str) {
        let def_key = if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let inst = inst_arc.read().await;
            inst.definition_key
        } else {
            return;
        };
        
        let bound_timers: Vec<String> = if let Some(def) = self.definitions.get(&def_key).await {
            def.nodes.iter()
                .filter_map(|(id, node)| {
                    if let BpmnElement::BoundaryTimerEvent { attached_to, .. } = node {
                        if attached_to == task_node_id {
                            Some(id.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        
        self.pending_timers.retain(|t| !(t.instance_id == instance_id && bound_timers.contains(&t.node_id)));
    }

    /// Deletes a process instance and cleans up associated pending tasks.
    pub async fn delete_instance(&mut self, instance_id: Uuid) -> EngineResult<()> {
        let removed_inst_arc = self.instances.remove(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        let removed_inst = removed_inst_arc.read().await.clone();

        if let Some(ref persistence) = self.persistence {
            // Delete associated files
            for value in removed_inst.variables.values() {
                if let Some(file_ref) = FileReference::from_variable_value(value) {
                    let _ = persistence.delete_file(&file_ref.object_key).await;
                }
            }

            // Delete associated user tasks from persistence
            for task in self.pending_user_tasks.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_user_task(task.task_id).await;
            }
            // Delete associated service tasks from persistence
            for task in self.pending_service_tasks.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_service_task(task.id).await;
            }
            // Delete associated timers from persistence
            for timer in self.pending_timers.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_timer(timer.id).await;
            }
            // Delete associated message catches from persistence
            for catch in self.pending_message_catches.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_message_catch(catch.id).await;
            }
            // Delete instance from persistence
            persistence.delete_instance(&instance_id.to_string()).await?;
        }

        // Clean up pending user tasks in memory
        self.pending_user_tasks.retain(|t| t.instance_id != instance_id);
        
        // Clean up pending service tasks in memory
        self.pending_service_tasks.retain(|t| t.instance_id != instance_id);

        // Clean up pending timers in memory
        self.pending_timers.retain(|t| t.instance_id != instance_id);

        // Clean up pending message catches in memory
        self.pending_message_catches.retain(|t| t.instance_id != instance_id);

        Ok(())
    }

    /// Deletes a process definition. 
    /// If cascade is true, deletes all associated process instances first.
    pub async fn delete_definition(&mut self, definition_key: Uuid, cascade: bool) -> EngineResult<()> {
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
                return Err(EngineError::DefinitionHasInstances(associated_instances.len()));
            }
            // Cascade delete instances
            for instance_id in associated_instances {
                self.delete_instance(instance_id).await?;
            }
        }

        self.definitions.remove(&definition_key).await;

        if let Some(ref persistence) = self.persistence {
            persistence.delete_definition(&definition_key.to_string()).await?;
        }

        Ok(())
    }

    /// Updates variables on a running process instance.
    ///
    /// - Keys with non-null values are created or overwritten.
    /// - Keys with `Value::Null` are removed from the instance variables.
    pub async fn update_instance_variables(
        &mut self,
        instance_id: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<()> {
        let old_state = if let Some(lk) = self.instances.get(&instance_id).await { Some(lk.read().await.clone()) } else { None };

        let updated_vars = {
            let instance_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut instance = instance_arc.write().await;

            let mut added: usize = 0;
            let mut modified: usize = 0;
            let mut deleted: usize = 0;

            for (key, value) in variables {
                if value.is_null() {
                    // Delete
                    if instance.variables.remove(&key).is_some() {
                        deleted += 1;
                    }
                } else {
                    match instance.variables.entry(key) {
                        std::collections::hash_map::Entry::Occupied(mut e) => {
                            // Update existing
                            e.insert(value);
                            modified += 1;
                        }
                        std::collections::hash_map::Entry::Vacant(e) => {
                            // Create new
                            e.insert(value);
                            added += 1;
                        }
                    }
                }
            }

            instance.audit_log.push(format!(
                "Variables updated: +{added} ~{modified} -{deleted}"
            ));

            log::info!(
                "Instance {}: variables updated (+{added} ~{modified} -{deleted})",
                instance_id
            );
            
            instance.variables.clone()
        };

        // Sync token variables in pending tasks so they don't overwrite changes on completion
        let shared_vars = std::sync::Arc::new(updated_vars);
        let mut user_task_ids = Vec::new();
        for task in &mut self.pending_user_tasks {
            if task.instance_id == instance_id {
                task.token.variables = (*shared_vars).clone();
                user_task_ids.push(task.task_id);
            }
        }

        let mut service_task_ids = Vec::new();
        for task in &mut self.pending_service_tasks {
            if task.instance_id == instance_id {
                task.token.variables = (*shared_vars).clone();
                service_task_ids.push(task.id);
            }
        }

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::VariableUpdated,
            "Variables updated directly",
            crate::history::ActorType::User, // API call
            None,
            old_state.as_ref()
        ).await;

        self.persist_instance(instance_id).await;

        for tid in user_task_ids {
            self.persist_user_task(tid).await;
        }
        for sid in service_task_ids {
            self.persist_service_task(sid).await;
        }

        Ok(())
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
