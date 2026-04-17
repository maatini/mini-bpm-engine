use std::collections::HashMap;
use std::sync::Arc;

use engine_core::WorkflowEngine;
use engine_core::WorkflowPersistence;
use persistence_nats::NatsPersistence;
use uuid::Uuid;

pub struct RestoreStats {
    pub definitions: usize,
    pub instances: usize,
    pub user_tasks: usize,
    pub service_tasks: usize,
    pub timers: usize,
    pub message_catches: usize,
}

pub struct StartupCoordinator {
    nats: Arc<NatsPersistence>,
}

impl StartupCoordinator {
    pub fn new(nats: Arc<NatsPersistence>) -> Self {
        Self { nats }
    }

    pub async fn restore(
        &self,
        engine: &mut WorkflowEngine,
        deployed_xml: &mut HashMap<String, String>,
    ) -> RestoreStats {
        let definitions = self.restore_definitions(engine, deployed_xml).await;
        let instances = self.restore_instances(engine).await;
        let user_tasks = self.restore_user_tasks(engine).await;
        let service_tasks = self.restore_service_tasks(engine).await;
        let timers = self.restore_timers(engine).await;
        let message_catches = self.restore_message_catches(engine).await;

        RestoreStats {
            definitions,
            instances,
            user_tasks,
            service_tasks,
            timers,
            message_catches,
        }
    }

    async fn restore_definitions(
        &self,
        engine: &mut WorkflowEngine,
        deployed_xml: &mut HashMap<String, String>,
    ) -> usize {
        let ids = match self.nats.list_bpmn_xml_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("Definitionen aus NATS laden fehlgeschlagen: {:?}", e);
                return 0;
            }
        };
        let mut count = 0;
        for nats_key in &ids {
            match self.nats.load_bpmn_xml(nats_key).await {
                Ok(xml) => match bpmn_parser::parse_bpmn_xml(&xml) {
                    Ok(mut def) => {
                        if let Ok(old_uuid) = Uuid::parse_str(nats_key) {
                            def.key = old_uuid;
                        }
                        let (key, _) = engine.deploy_definition(def).await;
                        deployed_xml.insert(key.to_string(), xml);
                        tracing::info!("Definition wiederhergestellt (key: {})", key);
                        count += 1;
                    }
                    Err(e) => tracing::error!("BPMN '{}' parsen fehlgeschlagen: {:?}", nats_key, e),
                },
                Err(e) => tracing::error!("XML für '{}' laden fehlgeschlagen: {:?}", nats_key, e),
            }
        }
        tracing::info!("Restore abgeschlossen: {count} Definition(en).");
        count
    }

    async fn restore_instances(&self, engine: &mut WorkflowEngine) -> usize {
        match self.nats.list_instances().await {
            Ok(instances) => {
                let num = instances.len();
                for inst in instances {
                    engine.restore_instance(inst).await;
                }
                tracing::info!("{num} Prozessinstanz(en) wiederhergestellt.");
                num
            }
            Err(e) => {
                tracing::error!("Instanzen aus NATS laden fehlgeschlagen: {:?}", e);
                0
            }
        }
    }

    async fn restore_user_tasks(&self, engine: &mut WorkflowEngine) -> usize {
        match self.nats.list_user_tasks().await {
            Ok(tasks) => {
                let num = tasks.len();
                for task in tasks {
                    engine.restore_user_task(task);
                }
                tracing::info!("{num} User-Task(s) wiederhergestellt.");
                num
            }
            Err(e) => {
                tracing::error!("User-Tasks aus NATS laden fehlgeschlagen: {:?}", e);
                0
            }
        }
    }

    async fn restore_service_tasks(&self, engine: &mut WorkflowEngine) -> usize {
        match self.nats.list_service_tasks().await {
            Ok(tasks) => {
                let num = tasks.len();
                for task in tasks {
                    engine.restore_service_task(task);
                }
                tracing::info!("{num} Service-Task(s) wiederhergestellt.");
                num
            }
            Err(e) => {
                tracing::error!("Service-Tasks aus NATS laden fehlgeschlagen: {:?}", e);
                0
            }
        }
    }

    async fn restore_timers(&self, engine: &mut WorkflowEngine) -> usize {
        match self.nats.list_timers().await {
            Ok(timers) => {
                let num = timers.len();
                for timer in timers {
                    engine.restore_timer(timer);
                }
                tracing::info!("{num} Timer wiederhergestellt.");
                num
            }
            Err(e) => {
                tracing::error!("Timer aus NATS laden fehlgeschlagen: {:?}", e);
                0
            }
        }
    }

    async fn restore_message_catches(&self, engine: &mut WorkflowEngine) -> usize {
        match self.nats.list_message_catches().await {
            Ok(catches) => {
                let num = catches.len();
                for catch in catches {
                    engine.restore_message_catch(catch);
                }
                tracing::info!("{num} Message-Catch(es) wiederhergestellt.");
                num
            }
            Err(e) => {
                tracing::error!("Message-Catches aus NATS laden fehlgeschlagen: {:?}", e);
                0
            }
        }
    }
}
