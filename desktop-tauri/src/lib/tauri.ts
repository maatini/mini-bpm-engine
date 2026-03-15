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
  definition_id: string;
  state: 'Running' | 'Completed' | { WaitingOnUserTask: { task_id: string } };
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

export async function startInstance(defId: string): Promise<string> {
  return invoke('start_instance', { defId });
}

export async function getPendingTasks(): Promise<PendingUserTask[]> {
  return invoke('get_pending_tasks');
}

export async function completeTask(taskId: string): Promise<void> {
  return invoke('complete_task', { taskId });
}

export async function listInstances(): Promise<ProcessInstance[]> {
  return invoke('list_instances');
}

export async function getInstanceDetails(instanceId: string): Promise<ProcessInstance> {
  return invoke('get_instance_details', { instanceId });
}
