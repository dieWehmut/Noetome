export interface ProjectSummary {
  path: string;
  project_id: string;
  schema_version: number;
  created_at: string;
  state: string;
  database: string;
}

export interface Failure { code: string; message: string }
export type Selection = { kind: "text"; ordinal: number } | { kind: "pdf"; page_index: number; region_id: string };
export interface SourceBinding {
  project_id: string; import_id: string; source_id: string; version_id: string;
  content_sha256: string; normalized_sha256: string | null; parse_id: string | null; manifest_sha256: string | null;
}
export type Locator = { kind: "text"; span_id: string; ordinal: number; resource_sha256: string }
  | { kind: "pdf"; parse_id: string; page_index: number; region_id: string; resource_id: string; resource_sha256: string; bbox: [number, number, number, number] };
export interface SourceImport { record: unknown; failure: Failure | null; project: ProjectSummary | null }
export interface SourceChoice {
  selection: Selection;
  label: string;
  byte_length: number;
  source_status: string;
  selectable: boolean;
  reason?: string | null;
}
export interface SourceChoices {
  schema_version: number;
  binding: SourceBinding | null;
  source_status: string;
  filename: string;
  choices: SourceChoice[];
}
export interface SourceUnits {
  binding: SourceBinding;
  units: { unit_id: string; locator: Locator; source_status: string; start_byte: number; end_byte: number; text: string }[];
}
export interface ExtractionResult { record: unknown; failure: Failure | null; output_root: string }
export interface PreviewExport {
  producer: "extraction_preview";
  status: "pending";
  output_root: string;
  vault_path: string;
  counts: { concepts: number; atoms: number; evidence: number };
  markdown: string;
}
export interface SelfTestResult extends ExtractionResult { vault_path: string | null }
export interface ModelStatus {
  ready: boolean;
  endpoint: string;
  model: string;
  credential_state: string;
  ledger_state: string;
  accounted_tokens: number | null;
  pending_reserved_tokens: number | null;
  remaining_tokens: number | null;
  model_attempts: number | null;
  message: string;
}
export function recordField(record: unknown, field: string): unknown {
  return record !== null && typeof record === "object" ? (record as Record<string, unknown>)[field] : undefined;
}
export function recordText(record: unknown, field: string): string {
  const value = recordField(record, field);
  return typeof value === "string" ? value : "";
}
export function localError(cause: unknown): string {
  if (typeof cause === "string" && cause.startsWith("{")) {
    try {
      const detail = JSON.parse(cause);
      if (typeof detail.code === "string" && typeof detail.message === "string") {
        return `${detail.code}：${detail.message}（退出码：${detail.exit_code ?? "未知"}）\n${detail.stderr ?? ""}`;
      }
    } catch { /* Preserve existing handling for ordinary errors. */ }
  }
  return typeof cause === "string" ? cause : cause instanceof Error ? cause.message : "本地组件未返回可用结果。";
}
export function failureText(failure: Failure): string { return `${failure.code}：${failure.message}`; }
export function selectionKey(selection: Selection): string { return JSON.stringify(selection); }
export function bindingKey(value: unknown): string {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return "";
  const record = value as Record<string, unknown>;
  return JSON.stringify(Object.keys(record).sort().map((key) => [key, record[key]]));
}
