import type { Workbench } from "./useWorkbench";

const tokens = (value: number | null | undefined) => value == null ? "不可得" : value.toLocaleString();
const ledgerLabel: Record<string, string> = { production_ready: "生产累计预算", production_invalid: "生产预算未通过校验", ready: "原有累计账本", missing: "累计账本缺失", invalid: "账本未通过校验", unavailable: "不可用" };
export default function AppModel({ workbench: w }: { workbench: Workbench }) {
  const model = w.model;
  return <section className="settings-section" aria-labelledby="model-settings-title" aria-busy={w.modelBusy}>
    <h2 id="model-settings-title" tabIndex={-1}>模型与累计预算</h2><p className="settings-description">查看当前服务状态，并通过本机加密凭据入口设置 API key。</p>
    <dl className="runtime-details model-details"><div><dt>模型</dt><dd>{model?.model || "尚未读取"}</dd></div><div><dt>服务地址</dt><dd>{model?.endpoint || "尚未读取"}</dd></div><div><dt>凭据状态</dt><dd>{model?.credential_state || "尚未读取"}</dd></div><div><dt>预算账本</dt><dd>{model ? ledgerLabel[model.ledger_state] ?? model.ledger_state : "尚未读取"}</dd></div><div><dt>累计已用 Token</dt><dd>{tokens(model?.accounted_tokens)}</dd></div><div><dt>待结算预留 Token</dt><dd>{tokens(model?.pending_reserved_tokens)}</dd></div><div><dt>当前可用 Token</dt><dd>{tokens(model?.remaining_tokens)}</dd></div><div><dt>模型尝试记录</dt><dd>{tokens(model?.model_attempts)}</dd></div></dl>
    <p className="settings-status" role="status">{model?.message || "尚未读取模型状态。"}</p>
    <div className="runtime-actions"><button className="button button-secondary" disabled={w.modelBusy} onClick={() => void w.refreshModel()}>刷新模型状态</button><button className="button button-primary" disabled={w.modelBusy || w.sourceBusy !== null} onClick={() => void w.modelCommand("model_credential_store")}>打开加密凭据设置</button></div>
    <p className="workflow-note">API key 在系统遮蔽输入窗口中填写并保存在本机加密文件中；此页面不会接收或保存明文 key。</p>
    <p className="workflow-note">整书整理沿用原有累计预算和完整历史，自动分批请求共用同一额度。当前可用额度已经扣除待核算预留；中断且没有完整用量回执的请求继续保留预留。生产预算存在时，这里读取当前累计状态。</p>
    <p className="workflow-note">“文本与旧资料”的小样例入口保留历史单请求规则。整书任务请从资料库或任务中心发起。</p>
    {model?.ledger_state === "missing" && <div className="budget-restore"><h3>累计账本缺失</h3><p>可按 2026-11-06 已授权快照恢复：累计已用 33,185 Token，10 条历史记录。恢复不会把预算重置为全新额度。</p><label className="workflow-consent"><input type="checkbox" checked={w.restoreConfirmed} disabled={w.modelBusy || w.sourceBusy !== null} onChange={(event) => w.setRestoreConfirmed(event.target.checked)} /><span>我确认 2026-11-06 快照之后没有额外模型调用，同意恢复该历史快照。</span></label><button className="button button-secondary" disabled={w.modelBusy || w.sourceBusy !== null || !w.restoreConfirmed} onClick={() => void w.modelCommand("model_budget_restore")}>按历史快照恢复账本</button></div>}
    {w.modelError && <p className="error-message" role="alert">{w.modelError}</p>}
  </section>;
}
