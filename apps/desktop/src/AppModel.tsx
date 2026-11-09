import type { Workbench } from "./useWorkbench";

const tokens = (value: number | null | undefined) => value == null ? "不可得" : value.toLocaleString();
export default function AppModel({ workbench: w }: { workbench: Workbench }) {
  const model = w.model;
  return <section className="settings-section" aria-labelledby="model-settings-title" aria-busy={w.modelBusy}>
    <h2 id="model-settings-title" tabIndex={-1}>模型与累计预算</h2><p className="settings-description">查看当前服务状态，并通过本机加密凭据入口设置 API key。</p>
    <dl className="runtime-details model-details"><div><dt>模型</dt><dd>{model?.model || "尚未读取"}</dd></div><div><dt>服务地址</dt><dd>{model?.endpoint || "尚未读取"}</dd></div><div><dt>凭据状态</dt><dd>{model?.credential_state || "尚未读取"}</dd></div><div><dt>预算账本</dt><dd>{model?.ledger_state || "尚未读取"}</dd></div><div><dt>累计已用 Token</dt><dd>{tokens(model?.accounted_tokens)}</dd></div><div><dt>待结算预留 Token</dt><dd>{tokens(model?.pending_reserved_tokens)}</dd></div><div><dt>剩余 Token</dt><dd>{tokens(model?.remaining_tokens)}</dd></div><div><dt>模型尝试记录</dt><dd>{tokens(model?.model_attempts)}</dd></div></dl>
    <p className="settings-status" role="status">{model?.message || "尚未读取模型状态。"}</p>
    <div className="runtime-actions"><button className="button button-secondary" disabled={w.modelBusy} onClick={() => void w.refreshModel()}>刷新模型状态</button><button className="button button-primary" disabled={w.modelBusy || w.sourceBusy !== null} onClick={() => void w.modelCommand("model_credential_store")}>打开加密凭据设置</button></div>
    <p className="workflow-note">API key 在系统遮蔽输入窗口中填写并保存在本机加密文件中；此页面不会接收或保存明文 key。</p>
    {model?.ledger_state === "missing" && <div className="budget-restore"><h3>累计账本缺失</h3><p>可按 2026-11-06 已授权快照恢复：累计已用 33,185 Token，10 条历史记录。恢复不会把预算重置为全新额度。</p><label className="workflow-consent"><input type="checkbox" checked={w.restoreConfirmed} disabled={w.modelBusy || w.sourceBusy !== null} onChange={(event) => w.setRestoreConfirmed(event.target.checked)} /><span>我确认 2026-11-06 快照之后没有额外模型调用，同意恢复该历史快照。</span></label><button className="button button-secondary" disabled={w.modelBusy || w.sourceBusy !== null || !w.restoreConfirmed} onClick={() => void w.modelCommand("model_budget_restore")}>按历史快照恢复账本</button></div>}
    {w.modelError && <p className="error-message" role="alert">{w.modelError}</p>}
  </section>;
}
