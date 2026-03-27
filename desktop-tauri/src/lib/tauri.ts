import { invoke } from '@tauri-apps/api/tauri';

export interface PendingUserTask {
  task_id: string;
  instance_id: string;
  node_id: string;
  assignee: string;
  created_at: string;
}

export interface ProcessInstance {
  id: string;
  definition_key: string;
  business_key: string;
  state: 'Running' | 'Completed' | { WaitingOnUserTask: { task_id: string } } | { WaitingOnServiceTask: { task_id: string } };
  current_node: string;
  audit_log: string[];
  variables: Record<string, unknown>;
}

export async function deploySimpleProcess(): Promise<string> {
  return invoke('deploy_simple_process');
}

export async function deployDefinition(xml: string, name: string): Promise<string> {
  return invoke('deploy_definition', { xml, name });
}

export async function startInstance(defId: string, variables?: Record<string, unknown>): Promise<string> {
  return invoke('start_instance', { defId, variables: variables || null });
}

export async function getPendingTasks(): Promise<PendingUserTask[]> {
  return invoke('get_pending_tasks');
}

export async function completeTask(taskId: string): Promise<void> {
  return invoke('complete_task', { taskId });
}

export interface PendingServiceTask {
  id: string;
  instance_id: string;
  definition_key: string;
  node_id: string;
  topic: string;
  worker_id: string | null;
  lock_expiration: string | null;
  retries: number;
  error_message: string | null;
  error_details: string | null;
  created_at: string;
}

export async function getPendingServiceTasks(): Promise<PendingServiceTask[]> {
  return invoke('get_pending_service_tasks');
}

export async function completeServiceTask(taskId: string, workerId: string, variables?: Record<string, unknown>): Promise<void> {
  return invoke('complete_service_task', { taskId, workerId, variables: variables || null });
}

export async function listInstances(): Promise<ProcessInstance[]> {
  return invoke('list_instances');
}

export async function getInstanceDetails(instanceId: string): Promise<ProcessInstance> {
  return invoke('get_instance_details', { instanceId });
}

export async function updateInstanceVariables(instanceId: string, variables: Record<string, unknown>): Promise<void> {
  return invoke('update_instance_variables', { instanceId, variables });
}

export interface DefinitionInfo {
  key: string;
  bpmn_id: string;
  node_count: number;
}

export async function listDefinitions(): Promise<DefinitionInfo[]> {
  return invoke('list_definitions');
}

export async function getDefinitionXml(definitionId: string): Promise<string> {
  return invoke('get_definition_xml', { definitionId });
}

export async function deleteInstance(instanceId: string): Promise<void> {
  return invoke('delete_instance', { instanceId });
}

export async function deleteDefinition(definitionId: string, cascade: boolean = false): Promise<void> {
  return invoke('delete_definition', { definitionId, cascade });
}

// ---------------------------------------------------------------------------
// Backend info & switching
// ---------------------------------------------------------------------------

export interface BackendInfo {
  backend_type: 'in-memory' | 'nats' | 'http';
  nats_url: string | null;
  connected: boolean;
}

export async function getBackendInfo(): Promise<BackendInfo> {
  return invoke('get_backend_info');
}

export async function switchBackend(backendType: string, natsUrl?: string): Promise<BackendInfo> {
  return invoke('switch_backend', { backendType, natsUrl: natsUrl ?? null });
}

// ---------------------------------------------------------------------------
// Monitoring
// ---------------------------------------------------------------------------

export interface NatsServerInfo {
  server_name: string;
  version: string;
  host: string;
  port: number;
  memory_bytes: number;
  storage_bytes: number;
  streams: number;
  consumers: number;
}

export interface MonitoringData {
  definitions_count: number;
  instances_total: number;
  instances_running: number;
  instances_completed: number;
  pending_user_tasks: number;
  pending_service_tasks: number;
  nats_server: NatsServerInfo | null;
}

export async function getMonitoringData(): Promise<MonitoringData> {
  return invoke('get_monitoring_data');
}

// ---------------------------------------------------------------------------
// Read BPMN file from local filesystem
// ---------------------------------------------------------------------------

export async function readBpmnFile(path: string): Promise<string> {
  return invoke('read_bpmn_file', { path });
}
