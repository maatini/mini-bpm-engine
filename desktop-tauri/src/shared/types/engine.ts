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
  business_key: string | null;
  state: 'Running' | 'Completed'
    | { WaitingOnUserTask: { task_id: string } }
    | { WaitingOnServiceTask: { task_id: string } }
    | { WaitingOnTimer: { timer_id: string } }
    | { WaitingOnMessage: { message_name: string } }
    | { Suspended: { previous_state: ProcessInstance['state'] } };
  current_node: string;
  audit_log: string[];
  variables: Record<string, unknown>;
  started_at?: string | null;
  completed_at?: string | null;
}

export interface CompletedInstanceQuery {
  definition_key?: string;
  business_key?: string;
  from?: string;
  to?: string;
  state_filter?: 'completed' | 'error';
  limit?: number;
  offset?: number;
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
  variables_snapshot: Record<string, unknown>;
}

export interface HistoryEntry {
  id: string;
  instance_id: string;
  timestamp: string;
  event_type: string;
  node_id: string | null;
  description: string;
  actor_type: string;
  actor_id: string | null;
  diff: {
    changes: Record<string, any>;
    human_readable: string | null;
  } | null;
  context: Record<string, any>;
  metadata: Record<string, any> | null;
  definition_version: number | null;
  is_snapshot: boolean;
  full_state_snapshot: Record<string, any> | null;
}

export interface HistoryQuery {
  event_types?: string;
  actor_types?: string;
}

export interface DefinitionInfo {
  key: string;
  bpmn_id: string;
  version: number;
  node_count: number;
  is_latest: boolean;
}

export interface BucketInfo {
  name: string;
  bucket_type: string;
  entries: number;
  size_bytes: number;
}

export interface StorageInfo {
  backend_name: string;
  version: string;
  host: string;
  port: number;
  memory_bytes: number;
  storage_bytes: number;
  streams: number;
  consumers: number;
  buckets: BucketInfo[];
}

export interface MonitoringData {
  definitions_count: number;
  instances_total: number;
  instances_running: number;
  instances_completed: number;
  pending_user_tasks: number;
  pending_service_tasks: number;
  pending_timers: number;
  pending_message_catches: number;
  storage_info: StorageInfo | null;
}

export interface BucketEntry {
  key: string;
  size_bytes: number | null;
  created_at: string | null;
}

export interface BucketEntryDetail {
  key: string;
  data: string;
}

export interface MoveTokenRequest {
  target_node_id: string;
  variables?: Record<string, unknown>;
  cancel_current?: boolean;
}

export interface FileReference {
  type: 'file';
  object_key: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  uploaded_at: string;
}

export interface PendingTimer {
  id: string;
  instance_id: string;
  node_id: string;
  expires_at: string;
  token_id: string;
  timer_def: { Date: string } | { Duration: string } | { RepeatingInterval: { interval: string; repetitions: number | null } } | null;
  remaining_repetitions: number | null;
}

export interface PendingMessageCatch {
  id: string;
  instance_id: string;
  node_id: string;
  message_name: string;
  token_id: string;
}
