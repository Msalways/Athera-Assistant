export type Status =
  | "created"
  | "running"
  | "waiting_for_auth"
  | "waiting_for_approval"
  | "waiting_for_user"
  | "waiting_for_resolution"
  | "completed"
  | "failed"
  | "cancelled";
export type Risk =
  | "read_only"
  | "local_safe_write"
  | "external_write"
  | "sensitive"
  | "destructive";
export interface Tool {
  id: string;
  version: string;
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  output_schema?: Record<string, unknown> | null;
  connection_id: string;
  source_tool: string;
  risk: Risk;
  enabled: boolean;
  requires_auth: boolean;
  requires_network: boolean;
}
export interface Skill {
  id: string;
  version: string;
  name: string;
  description: string;
  instructions: string;
  tool_requirements: string[];
  enabled: boolean;
}
export type Capability =
  { kind: "tool"; spec: Tool } | { kind: "skill"; spec: Skill };
export interface Task {
  id: string;
  input: { conversation_id: string; text: string; source: "text" | "voice" };
  status: Status;
  role: string;
  plan: string[];
  step: number;
  message: string;
  output?: AssistantOutput | null;
  result_refs: string[];
  pending: {
    id: string;
    call: {
      tool_id: string;
      version: string;
      arguments: Record<string, unknown>;
    };
    spec: Tool;
    approved: boolean;
    started: boolean;
  } | null;
}
export interface AssistantOutput {
  schema: "aethra.output.v1";
  blocks: OutputBlock[];
}
export type OutputBlock =
  | { type: "markdown"; id: string; markdown: string }
  | { type: "list"; id: string; ordered: boolean; items: string[] }
  | { type: "table"; id: string; columns: string[]; rows: string[][] }
  | {
      type: "sources";
      id: string;
      retrieved_at: number;
      partial: boolean;
      citations_resolved: boolean;
      sources: { id: string; title: string; url: string; excerpt: string }[];
    };
export type RunEventKind =
  | "run_started"
  | "task_state"
  | "worker_started"
  | "text_delta"
  | "worker_terminal"
  | "output_upsert"
  | "run_paused"
  | "run_terminal";
export interface RunEventEnvelope {
  schema: "aethra.run-event.v1";
  event_id: string;
  sequence: number;
  run_id: string;
  task_id: string;
  worker_id: string | null;
  kind: RunEventKind;
  status: Status;
  step: number;
  message: string;
  text_delta: string | null;
  output: AssistantOutput | null;
}
export interface RunEventPage {
  schema: "aethra.run-events.v1";
  after: number;
  next_after: number;
  has_more: boolean;
  reset_required: boolean;
  events: RunEventEnvelope[];
}
export interface Settings {
  cloud: {
    id: string;
    endpoint: string;
    model: string;
    secret_ref: string;
    api: "responses" | "chat_completions";
    max_output_tokens?: number;
  } | null;
  engine: {
    max_steps: number;
    max_failures: number;
    max_identical_calls: number;
    timeout_seconds: number;
    candidate_limit: number;
    tool_limit: number;
    fast_context_bytes: number;
    cloud_context_bytes: number;
  };
  connections: {
    schema: "aethra.mcp-connection.v1";
    id: string;
    name: string;
    url: string;
    transport: "streamable_http";
    authentication:
      | "none"
      | { kind: "none" }
      | { kind: "bearer_token"; secret_ref: string }
      | { kind: "api_key_header"; header: string; secret_ref: string }
      | {
          kind: "oauth_authorization_code";
          token_ref: string;
          authorization_server: string;
          resource: string;
          requested_scopes: string[];
        };
    preset?: "parallel_search" | null;
  }[];
}
export interface Snapshot {
  cloud_session_key: boolean;
  cloud_credential: "not_configured" | "missing" | "configured";
  tasks: Task[];
  capabilities: Capability[];
  settings: Settings;
  needle: string;
  voice: string;
  local_chat?: ProviderAvailability;
}

export interface OAuthAuthorizationStart {
  transaction_id: string;
  connection_id: string;
  authorization_url: string;
  expires_at: number;
  requested_scopes: string[];
  resume_task_id: string | null;
}

export interface OAuthConnectionStatus {
  connection_id: string;
  state: "missing" | "waiting_for_user_authorization" | "connected" | "expired";
  granted_scopes: string[];
  expires_at: number | null;
}

export type MessageStatus =
  "generating" | "complete" | "cancelled" | "failed" | "interrupted";
export type ProviderAvailability =
  "missing_model" | "ready" | "busy" | "unavailable";
export interface Conversation {
  id: string;
  title: string;
  temporary: boolean;
  summary: string;
  updated_at: number;
}
export interface Message {
  id: string;
  conversation_id: string;
  role: "user" | "assistant";
  content: string;
  status: MessageStatus;
  created_at: number;
}
export interface PersonalMemory {
  id: string;
  text: string;
  updated_at: number;
}
export interface ModelManifest {
  id: string;
  url: string;
  size_bytes: number;
  sha256: string;
  revision: string;
  license: string;
  runtime_revision: string;
  context_tokens: number;
}
export interface ModelInstallation {
  manifest: ModelManifest;
  status:
    | "missing"
    | "downloading"
    | "verifying"
    | "installed"
    | "failed"
    | "cancelled";
  downloaded_bytes: number;
  error: string | null;
}
export interface ModelStatus {
  availability: ProviderAvailability;
  installation: ModelInstallation | null;
  manifest: ModelManifest | null;
}
export interface SourceReference {
  id: string;
  title: string;
  url: string | null;
  excerpt: string;
}
export interface NoteRevision {
  revision: number;
  text: string;
  created_at: number;
}
export interface ResearchSession {
  id: string;
  conversation_id: string;
  title: string;
  sources: SourceReference[];
  hypotheses: string[];
  decisions: string[];
  experiments: string[];
  notes: NoteRevision[];
}
