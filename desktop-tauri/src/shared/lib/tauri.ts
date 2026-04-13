import { invoke } from '@tauri-apps/api/core';
import type {
  PendingUserTask,
  ProcessInstance,
  PendingServiceTask,
  HistoryEntry,
  HistoryQuery,
  DefinitionInfo,
  MonitoringData,
  BucketEntry,
  BucketEntryDetail,
  FileReference,
  MoveTokenRequest,
  PendingTimer,
  PendingMessageCatch,
  CompletedInstanceQuery
} from '../types/engine';

export * from '../types/engine';

export async function deploySimpleProcess(): Promise<string> {
  return invoke('deploy_simple_process');
}

export async function deployDefinition(xml: string, name: string): Promise<string> {
  return invoke('deploy_definition', { xml, name });
}

export async function startInstance(defId: string, variables?: Record<string, unknown>): Promise<string> {
  return invoke('start_instance', { defId, variables: variables || null });
}

export async function startTimerInstance(defId: string, variables?: Record<string, unknown>): Promise<string> {
  return invoke('start_timer_instance', { defId, variables: variables || null });
}

export async function getPendingTasks(): Promise<PendingUserTask[]> {
  return invoke('get_pending_tasks');
}

export async function completeTask(taskId: string, variables?: Record<string, unknown>): Promise<void> {
  return invoke('complete_task', { taskId, variables: variables || null });
}

export async function getPendingServiceTasks(): Promise<PendingServiceTask[]> {
  return invoke('get_pending_service_tasks');
}

export async function fetchAndLockServiceTasks(workerId: string, maxTasks: number, topicName: string, lockDuration: number): Promise<PendingServiceTask[]> {
  return invoke('fetch_and_lock_service_tasks', { workerId, maxTasks, topicName, lockDuration });
}

export async function completeServiceTask(taskId: string, workerId: string, variables?: Record<string, unknown>): Promise<void> {
  return invoke('complete_service_task', { taskId, workerId, variables: variables || null });
}

export async function retryIncident(taskId: string, retries?: number): Promise<void> {
  return invoke('retry_incident', { taskId, retries: retries ?? null });
}

export async function resolveIncident(taskId: string, variables?: Record<string, unknown>): Promise<void> {
  return invoke('resolve_incident', { taskId, variables: variables ?? null });
}

export async function listInstances(): Promise<ProcessInstance[]> {
  return invoke('list_instances');
}

export async function getInstanceDetails(instanceId: string): Promise<ProcessInstance> {
  return invoke('get_instance_details', { instanceId });
}

export async function getInstanceHistory(instanceId: string, query?: HistoryQuery): Promise<HistoryEntry[]> {
  return invoke('get_instance_history', { 
    instanceId,
    eventTypes: query?.event_types || null,
    actorTypes: query?.actor_types || null
  });
}

export async function updateInstanceVariables(instanceId: string, variables: Record<string, unknown>): Promise<void> {
  return invoke('update_instance_variables', { instanceId, variables });
}

export async function listDefinitions(): Promise<DefinitionInfo[]> {
  return invoke('list_definitions');
}

export async function getDefinitionXml(definitionId: string): Promise<string> {
  return invoke('get_definition_xml', { definitionId });
}

export async function suspendInstance(instanceId: string): Promise<void> {
  return invoke('suspend_instance', { instanceId });
}

export async function resumeInstance(instanceId: string): Promise<void> {
  return invoke('resume_instance', { instanceId });
}

export async function moveToken(instanceId: string, request: MoveTokenRequest): Promise<void> {
  return invoke('move_token', {
    instanceId,
    targetNodeId: request.target_node_id,
    variables: request.variables ?? null,
    cancelCurrent: request.cancel_current ?? true,
  });
}

export async function deleteInstance(instanceId: string): Promise<void> {
  return invoke('delete_instance', { instanceId });
}

export async function deleteDefinition(definitionId: string, cascade: boolean = false): Promise<void> {
  return invoke('delete_definition', { definitionId, cascade });
}

export async function deleteAllDefinitions(bpmnId: string, cascade: boolean = false): Promise<void> {
  return invoke('delete_all_definitions', { bpmnId, cascade });
}

// ---------------------------------------------------------------------------
// Backend info & switching
// ---------------------------------------------------------------------------

export async function resetDatabase(): Promise<void> {
  return invoke('reset_database');
}

export async function correlateMessage(
  messageName: string,
  businessKey?: string,
  variables?: Record<string, unknown>
): Promise<string[]> {
  return invoke('correlate_message', { messageName, businessKey, variables });
}

export async function getApiUrl(): Promise<string> {
  return invoke('get_api_url');
}

export async function setApiUrl(url: string): Promise<void> {
  return invoke('set_api_url', { url });
}

// ---------------------------------------------------------------------------
// Monitoring
// ---------------------------------------------------------------------------

export async function getMonitoringData(): Promise<MonitoringData> {
  return invoke('get_monitoring_data');
}

export async function getBucketEntries(bucket: string, offset: number = 0, limit: number = 50): Promise<BucketEntry[]> {
  return invoke('get_bucket_entries', { bucket, offset, limit });
}

export async function getBucketEntryDetail(bucket: string, key: string): Promise<BucketEntryDetail> {
  return invoke('get_bucket_entry_detail', { bucket, key });
}

// ---------------------------------------------------------------------------
// Read BPMN file from local filesystem
// ---------------------------------------------------------------------------

export async function readBpmnFile(path: string): Promise<string> {
  return invoke('read_bpmn_file', { path });
}

// ---------------------------------------------------------------------------
// File Attachments
// ---------------------------------------------------------------------------

export async function uploadInstanceFile(
  instanceId: string, varName: string, filePath: string
): Promise<FileReference> {
  return invoke('upload_instance_file', { instanceId, varName, filePath });
}

export async function downloadInstanceFile(
  instanceId: string, varName: string, savePath: string
): Promise<void> {
  return invoke('download_instance_file', { instanceId, varName, savePath });
}

export async function deleteInstanceFile(
  instanceId: string, varName: string
): Promise<void> {
  return invoke('delete_instance_file', { instanceId, varName });
}

// ---------------------------------------------------------------------------
// Overview: Timers & Messages
// ---------------------------------------------------------------------------

export async function getPendingTimers(): Promise<PendingTimer[]> {
  return invoke('get_pending_timers');
}

export async function getPendingMessageCatches(): Promise<PendingMessageCatch[]> {
  return invoke('get_pending_message_catches');
}

// ---------------------------------------------------------------------------
// Historical (completed) instances
// ---------------------------------------------------------------------------

export async function queryCompletedInstances(query: CompletedInstanceQuery = {}): Promise<ProcessInstance[]> {
  return invoke('query_completed_instances', {
    definitionKey: query.definition_key ?? null,
    businessKey: query.business_key ?? null,
    from: query.from ?? null,
    to: query.to ?? null,
    stateFilter: query.state_filter ?? null,
    limit: query.limit ?? null,
    offset: query.offset ?? null,
  });
}

export async function getCompletedInstance(instanceId: string): Promise<ProcessInstance> {
  return invoke('get_completed_instance', { instanceId });
}
