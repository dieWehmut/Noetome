import type { Failure } from "./workbenchTypes";

export interface CorpusIssue extends Failure {}
export type CorpusStatus = "acquiring" | "interrupted" | "queued" | "running" | "paused" | "needs_review" | "ready" | "failed";
export interface CorpusSource {
  source_id: string;
  display_name: string;
  content_sha256: string | null;
  byte_length: number;
  status: CorpusStatus;
  page_count: number | null;
  processed_pages: number;
  text_pages: number;
  ocr_pages: number;
  needs_ocr_pages: number;
  review_pages: number;
  failed_pages: number;
  blank_pages: number;
  error: Failure | null;
  created_at: string;
}
export interface CorpusPreflight {
  schema_version: number;
  kind: "preflight";
  status: "ready" | "password_required" | "failed";
  byte_length: number;
  page_count: number | null;
  encrypted: boolean;
  password_required: boolean;
  text_sample: { page_number: number; status: string; text_characters: number; issues: CorpusIssue[] }[];
  outline: { level: number; title: string; page_number: number | null }[];
  limits: {
    max_file_bytes: number | null;
    max_pages: number | null;
    default_batch_pages: number;
    batch_timeout_seconds: number;
    max_page_json_bytes: number;
    ocr_available: boolean;
    timeout_managed_by_host: boolean;
  };
  parser: Record<string, string>;
  ocr?: {
    available: boolean;
    engine?: string;
    engine_version?: string;
    page_timeout_seconds?: number;
    render_dpi?: number;
    message?: string;
    error?: Failure;
    quality_status?: string;
  };
  issues: CorpusIssue[];
}
export interface CorpusRegion {
  ordinal: number;
  region_id?: string;
  kind: string;
  text: string;
  bbox: [number, number, number, number];
}
export interface CorpusPage {
  page_number: number;
  source_id?: string;
  page_revision?: string;
  parser?: Record<string, unknown>;
  status: string;
  text: string;
  regions: CorpusRegion[];
  issues: CorpusIssue[];
  geometry: unknown;
  width_pt?: number | null;
  height_pt?: number | null;
  rotation?: number | null;
}
export interface CorpusPages {
  source_id: string;
  offset: number;
  limit: number;
  total: number | null;
  pages: CorpusPage[];
}

export const corpusStatusLabel: Record<CorpusStatus, string> = {
  acquiring: "正在保存原件", interrupted: "获取已中断", queued: "等待解析", running: "解析中", paused: "已暂停",
  needs_review: "原文有待复核问题", ready: "原文解析完成", failed: "解析未完成",
};
export const corpusPageStatusLabel: Record<string, string> = {
  pending: "尚未处理", text_ready: "文字已保存", ocr_ready: "识别文字已保存",
  ready: "原文已保存", blank: "空白页", needs_ocr: "扫描内容待识别", needs_review: "待复核", failed: "解析失败",
};
export function byteSize(bytes: number): string {
  if (bytes < 1024) return `${bytes.toLocaleString()} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
}
