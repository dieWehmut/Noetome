import type { CorpusPage } from "./corpusTypes";
import type { Failure } from "./workbenchTypes";

export interface CompileSource { source_id: string; source_sha256: string; display_name: string; byte_length: number; physical_pages: number }
export interface CompileBudget { accounted_tokens: number; pending_reserved_tokens: number; remaining_tokens: number; model_attempts: number; stopped?: boolean }
export interface CompileRun {
  schema_version: number; run_id: string; status: string; stage: string; sources: CompileSource[];
  physical_pages: number; prepared_pages: number; units_total: number; units_confirmed: number; units_failed: number;
  pending_ocr_pages: number; unresolved_pages: number; concepts: number; atoms: number; evidence: number;
  attempts: number; ir_revision: number; knowledge_status: string; scientific_quality: string;
  budget: CompileBudget | null; error: Failure | null; created_at: string;
}
export type CompileKind = "concepts" | "atoms" | "evidence";
export type CompileControl = "pause" | "cancel" | "retry";
export interface CompileQuery { schema_version: number; run_id: string; kind: CompileKind; offset: number; limit: number; total: number; items: Record<string, unknown>[] }
export interface CompileEvidence { evidence: Record<string, unknown>; page: CorpusPage }
export const compileStatusLabel: Record<string, string> = {
  queued: "等待开始", preparing: "准备全部页面", running: "正在整理", paused: "已暂停", cancelled: "已取消",
  completed: "处理结束", completed_with_issues: "处理结束，仍有问题", needs_review: "等待复核", failed: "处理失败",
  budget_pending: "预算存在待核算用量", budget_exhausted: "预算不足", blocked: "等待解决问题",
  budget_stop: "预算等待处理", budget_stopped: "预算等待处理", ready: "处理结束，知识待核验",
  interrupted: "运行已中断，可检查并继续", succeeded: "处理结束，知识待核验",
};
export function isCompileActive(run: CompileRun | null) { return !!run && ["running", "preparing"].includes(run.status); }
export function compileNeedsRetry(run: CompileRun | null) { return run?.error != null; }
export function textField(row: Record<string, unknown>, name: string) { return typeof row[name] === "string" ? row[name] as string : ""; }
export function stringList(row: Record<string, unknown>, name: string) { return Array.isArray(row[name]) ? (row[name] as unknown[]).filter((value): value is string => typeof value === "string") : []; }
export function evidenceIds(row: Record<string, unknown>) {
  return Array.isArray(row.evidence_links) ? row.evidence_links.flatMap((value: unknown) => value && typeof value === "object" && typeof (value as Record<string, unknown>).evidence_id === "string" ? [(value as Record<string, string>).evidence_id] : []) : [];
}
