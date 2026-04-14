import "@typespec/http";
import "@typespec/rest";
import "@typespec/openapi3";

using TypeSpec.Http;
using TypeSpec.Rest;
using TypeSpec.OpenAPI;

@service(#{
  title: "bpmninja Engine Server API"
})
@server("http://localhost:8081", "Local Engine Server")
namespace MiniBpm;

// --------------------------------------------------------------------------
// Models
// --------------------------------------------------------------------------

@error
model AppError {
  error: string;
}

model VariableValue {
  type: string;
  value?: unknown;
  filename?: string;
  size_bytes?: int64;
  content_type?: string;
}

@doc("Lifecycle state. Terminal states are `Completed`, `CompletedWithError`, and `Terminated`. Waiting variants carry additional payloads in the JSON object form.")
model ProcessInstance {
  id: string;
  definition_key: string;
  business_key?: string;
  parent_instance_id?: string;
  state: unknown;
  current_node?: string;
  variables: Record<VariableValue>;
  started_at?: string;
  completed_at?: string;
}

model DeployRequest {
  @doc("BPMN 2.0 XML")
  xml: string;
  name: string;
}

model DeployResponse {
  definition_key: string;
}

model StartRequest {
  definition_key: string;
  business_key?: string;
  variables?: Record<unknown>;
}

model StartLatestRequest {
  bpmn_id: string;
  business_key?: string;
  variables?: Record<unknown>;
}

model StartResponse {
  instance_id: string;
}

model CompleteRequest {
  variables?: Record<unknown>;
}

model MoveTokenRequest {
  target_node_id: string;
}

model PendingUserTask {
  id: string;
  instance_id: string;
  node_id: string;
  state: "Pending" | "Completed";
  created_at: string;
}

model PendingServiceTask {
  id: string;
  instance_id: string;
  definition_key?: string;
  node_id: string;
  topic: string;
  state: string;
  locked_by?: string;
  lock_expires_at?: string;
  variables: Record<VariableValue>;
  retries?: int32;
  error_message?: string;
}

model TopicRequest {
  topicName: string;
  lockDuration: int64;
}

model FetchAndLockRequest {
  workerId: string;
  maxTasks: int32;
  topics: TopicRequest[];
  asyncResponseTimeout?: int64;
}

model CompleteServiceTaskRequest {
  workerId: string;
  variables?: Record<unknown>;
}

model FailServiceTaskRequest {
  workerId: string;
  retries?: int32;
  errorMessage?: string;
  errorDetails?: string;
}

model ExtendLockRequest {
  workerId: string;
  newDuration: int64;
}

model BpmnErrorRequest {
  workerId: string;
  errorCode: string;
}

model DefinitionInfo {
  key: string;
  bpmn_id: string;
  version: int32;
  node_count: int32;
}

model CorrelateMessageRequest {
  messageName: string;
  businessKey?: string;
  variables?: Record<unknown>;
}

model CorrelateMessageResponse {
  affectedInstances: string[];
}

model ProcessTimersResponse {
  triggered: int32;
}

model PendingTimer {
  instance_id: string;
  node_id: string;
  fire_at: string;
}

model PendingMessage {
  instance_id: string;
  node_id: string;
  message_name: string;
}

model BackendInfo {
  backend_type: string;
  nats_url?: string;
  connected: boolean;
}

model StorageInfo {
  host: string;
  port: int32;
  messages: int32;
  bytes: int64;
}

model MonitoringData {
  definitions_count: int32;
  instances_total: int32;
  instances_running: int32;
  instances_completed: int32;
  pending_user_tasks: int32;
  pending_service_tasks: int32;
  storage_info?: StorageInfo;
}

model BucketEntry {
  key: string;
  size_bytes?: int64;
  revision?: int64;
  updated_at?: string;
}

model BucketEntryDetail {
  key: string;
  value: unknown;
}

model HistoryEntry {
  id: string;
  instance_id: string;
  node_id?: string;
  event_type: string;
  actor_type: string;
  timestamp: string;
  details: unknown;
}

model UpdateVariablesRequest {
  variables: Record<unknown>;
}

model UploadFileRequest {
  file: HttpPart<bytes>;
}

// --------------------------------------------------------------------------
// Endpoints
// --------------------------------------------------------------------------

@route("/api")
namespace Api {

  // ---- Deployment & lifecycle ----

  @post @route("/deploy")
  op deploy(@body request: DeployRequest): DeployResponse | AppError;

  @post @route("/start")
  op start(@body request: StartRequest): StartResponse | AppError;

  @post @route("/start/latest")
  @doc("Start an instance of the latest deployed version of a BPMN definition (by bpmn_id).")
  op startLatest(@body request: StartLatestRequest): StartResponse | AppError;

  @post @route("/start/timer")
  @doc("Start an instance whose start event carries a timerEventDefinition.")
  op startTimer(@body request: StartRequest): StartResponse | AppError;

  // ---- User tasks ----

  @get @route("/tasks")
  op getTasks(): PendingUserTask[] | AppError;

  @post @route("/complete/{id}")
  op completeTask(@path id: string, @body request: CompleteRequest): { @statusCode statusCode: 204 } | AppError;

  // ---- Active instances ----

  @get @route("/instances")
  op listInstances(): ProcessInstance[] | AppError;

  @get @route("/instances/{id}")
  @doc("Returns an instance by id. Falls back to the history archive if the instance is no longer active.")
  op getInstance(@path id: string): ProcessInstance | AppError;

  @delete @route("/instances/{id}")
  op deleteInstance(@path id: string): { @statusCode statusCode: 204 } | AppError;

  @put @route("/instances/{id}/variables")
  op updateInstanceVariables(@path id: string, @body request: UpdateVariablesRequest): { @statusCode statusCode: 204 } | AppError;

  @post @route("/instances/{id}/suspend")
  op suspendInstance(@path id: string): { @statusCode statusCode: 204 } | AppError;

  @post @route("/instances/{id}/resume")
  op resumeInstance(@path id: string): { @statusCode statusCode: 204 } | AppError;

  @post @route("/instances/{id}/move-token")
  op moveToken(@path id: string, @body request: MoveTokenRequest): { @statusCode statusCode: 204 } | AppError;

  // ---- Files (variable-attached) ----

  @post @route("/instances/{id}/files/{var_name}")
  op uploadInstanceFile(
    @path id: string,
    @path var_name: string,
    @header contentType: "multipart/form-data",
    @multipartBody body: UploadFileRequest
  ): { @statusCode statusCode: 201 } | AppError;

  @get @route("/instances/{id}/files/{var_name}")
  op getInstanceFile(@path id: string, @path var_name: string): bytes | AppError;

  @delete @route("/instances/{id}/files/{var_name}")
  op deleteInstanceFile(@path id: string, @path var_name: string): { @statusCode statusCode: 204 } | AppError;

  // ---- Per-instance history ----

  @get @route("/instances/{id}/history")
  op getInstanceHistory(
    @path id: string,
    @query event_types?: string,
    @query node_id?: string,
    @query actor_type?: string,
    @query from?: string,
    @query to?: string,
    @query limit?: int32,
    @query offset?: int32
  ): HistoryEntry[] | AppError;

  @get @route("/instances/{id}/history/{event_id}")
  op getInstanceHistoryEntry(@path id: string, @path event_id: string): HistoryEntry | AppError;

  // ---- Archived (completed) instances ----

  @get @route("/history/instances")
  @doc("Query archived (completed) process instances.")
  op listCompletedInstances(
    @query definition_key?: string,
    @query business_key?: string,
    @query from?: string,
    @query to?: string,
    @doc("Terminal-state filter: `completed` or `error`.")
    @query state?: "completed" | "error",
    @query limit?: int32,
    @query offset?: int32
  ): ProcessInstance[] | AppError;

  @get @route("/history/instances/{id}")
  op getCompletedInstance(@path id: string): ProcessInstance | AppError;

  // ---- Definitions ----

  @get @route("/definitions")
  op listDefinitions(): DefinitionInfo[] | AppError;

  @get @route("/definitions/{id}/xml")
  op getDefinitionXml(@path id: string): string | AppError;

  @delete @route("/definitions/{id}")
  op deleteDefinition(@path id: string, @query cascade?: boolean): { @statusCode statusCode: 204 } | AppError;

  @delete @route("/definitions/bpmn/{bpmn_id}")
  @doc("Delete all versions of a definition by its bpmn_id.")
  op deleteAllDefinitions(@path bpmn_id: string, @query cascade?: boolean): { @statusCode statusCode: 204 } | AppError;

  // ---- Messages & timers ----

  @post @route("/message")
  op correlateMessage(@body request: CorrelateMessageRequest): CorrelateMessageResponse | AppError;

  @get @route("/timers")
  op getPendingTimers(): PendingTimer[] | AppError;

  @post @route("/timers/process")
  op processTimers(): ProcessTimersResponse | AppError;

  @get @route("/messages")
  op getPendingMessages(): PendingMessage[] | AppError;

  // ---- Service tasks (External-Task-Worker-kompatibel) ----

  @get @route("/service-tasks")
  op getServiceTasks(): PendingServiceTask[] | AppError;

  @post @route("/service-task/fetchAndLock")
  op fetchAndLockServiceTasks(@body request: FetchAndLockRequest): PendingServiceTask[] | AppError;

  @post @route("/service-task/{id}/complete")
  op completeServiceTask(@path id: string, @body request: CompleteServiceTaskRequest): { @statusCode statusCode: 204 } | AppError;

  @post @route("/service-task/{id}/failure")
  op failServiceTask(@path id: string, @body request: FailServiceTaskRequest): { @statusCode statusCode: 204 } | AppError;

  @post @route("/service-task/{id}/retry")
  op retryIncident(@path id: string): { @statusCode statusCode: 204 } | AppError;

  @post @route("/service-task/{id}/resolve")
  op resolveIncident(@path id: string): { @statusCode statusCode: 204 } | AppError;

  @post @route("/service-task/{id}/extendLock")
  op extendLock(@path id: string, @body request: ExtendLockRequest): { @statusCode statusCode: 204 } | AppError;

  @post @route("/service-task/{id}/bpmnError")
  op bpmnError(@path id: string, @body request: BpmnErrorRequest): { @statusCode statusCode: 204 } | AppError;

  // ---- Monitoring & health ----

  @get @route("/info")
  op getInfo(): BackendInfo | AppError;

  @get @route("/monitoring")
  op getMonitoring(): MonitoringData | AppError;

  @get @route("/monitoring/buckets/{bucket}/entries")
  op getBucketEntries(@path bucket: string): BucketEntry[] | AppError;

  @get @route("/monitoring/buckets/{bucket}/entries/{key}")
  op getBucketEntryDetail(@path bucket: string, @path key: string): BucketEntryDetail | AppError;

  @get @route("/health")
  op health(): { @statusCode statusCode: 200 } | AppError;

  @get @route("/ready")
  op ready(): { @statusCode statusCode: 200 } | AppError;
}
