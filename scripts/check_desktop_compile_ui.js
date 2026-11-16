// Playwright function. Run against `npm run web:dev -- --host 127.0.0.1 --port 1421`.
// This explicitly mocks IPC and performs zero real PDF/model/credential operations.
async (page) => {
  const checks = [], failures = [];
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const check = async (name, action) => { await action(); checks.push({ name, passed: true }); };
  page.on("pageerror", (error) => failures.push(String(error)));
  await page.context().addInitScript(() => {
    window.isTauri = true;
    const a = "C:\\synthetic\\project-a", b = "C:\\synthetic\\project-b";
    localStorage.setItem("noetome.last-project.v1", a);
    const source = (id, count) => ({ source_id: id, display_name: `Synthetic ${id}`, content_sha256: "a".repeat(64), byte_length: 80000, status: "needs_review", page_count: count, processed_pages: count, text_pages: count - 2, ocr_pages: 0, needs_ocr_pages: 1, review_pages: 2, failed_pages: 1, blank_pages: 0, error: null, created_at: "2026-11-16T00:00:00Z" });
    const summary = (project, sources) => ({ schema_version: 1, run_id: project === b ? "run_B" : "run_A", status: "queued", stage: "queued", sources: sources.map((id) => ({ source_id: id, source_sha256: "a".repeat(64), display_name: `Synthetic ${id}`, byte_length: 80000, physical_pages: id === "selected_A" ? 3648 : 100 })), physical_pages: sources.includes("selected_A") ? 3648 : 100, prepared_pages: 3648, units_total: 5000, units_confirmed: 0, units_failed: 1, pending_ocr_pages: 1, unresolved_pages: 2, concepts: 45, atoms: 45, evidence: 45, attempts: 0, ir_revision: 0, knowledge_status: "pending", scientific_quality: "not_measured", budget: { accounted_tokens: 33185, pending_reserved_tokens: 9000, remaining_tokens: 9957815, model_attempts: 10, stopped: false }, error: null, created_at: "2026-11-16T00:00:00Z" });
    const persisted = JSON.parse(localStorage.getItem("noetome.mock-compile-runs") || "{}");
    const save = () => localStorage.setItem("noetome.mock-compile-runs", JSON.stringify(persisted));
    const mock = window.__compileMock = { calls: [], pending: null, wrongQuery: false, deferQuery: false, releaseQuery: null,
      failRun: () => { persisted[a].status = "failed"; persisted[a].units_failed = 1; persisted[a].error = { code: "EXTRACT_TIMEOUT", message: "Synthetic request timed out; no real request" }; save(); } };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: async (command, args = {}) => {
        mock.calls.push({ command, args: JSON.parse(JSON.stringify(args)) });
        const project = args.project || a;
        const run = persisted[project];
        if (command === "project_open") return { path: args.path, project_id: args.path === b ? "project_B" : "project_A", schema_version: 3, created_at: "2026-11-16T00:00:00Z", state: "ready", database: "project.sqlite" };
        if (command === "model_status") return { ready: true, endpoint: "synthetic://mock", model: "fake-ui", credential_state: "ready", ledger_state: "production_ready", accounted_tokens: 33185, pending_reserved_tokens: 9000, remaining_tokens: 9957815, model_attempts: 10, message: "IPC mock; no requests" };
        if (command === "source_saved") return { project_id: project === b ? "project_B" : "project_A", sources: [] };
        if (command === "corpus_list") return project === b ? [source("selected_B", 100)] : [source("selected_A", 3648), source("unselected_private", 100)];
        if (command === "corpus_pages") return { source_id: args.sourceId, offset: args.offset, limit: args.limit, total: 3648, pages: [] };
        if (command === "compile_list") return run ? [run] : [];
        if (command === "compile_status") return JSON.parse(JSON.stringify(run));
        if (command === "compile_create") { persisted[project] = summary(project, args.sourceIds); save(); return persisted[project]; }
        if (command === "compile_run") {
          run.status = "running"; run.stage = "extracting"; run.attempts++; save();
          return await new Promise((resolve) => { mock.pending = () => resolve(JSON.parse(JSON.stringify(run))); });
        }
        if (command === "compile_control") {
          if (args.action === "retry") { run.status = "queued"; run.units_failed = 0; run.error = null; }
          else { run.status = args.action === "pause" ? "paused" : "cancelled"; if (mock.pending) { run.units_confirmed += 20; run.ir_revision++; } }
          save();
          if (args.action !== "retry" && mock.pending) { const resolve = mock.pending; mock.pending = null; setTimeout(resolve, 50); }
          return JSON.parse(JSON.stringify(run));
        }
        if (command === "compile_query") {
          const total = args.kind === "concepts" ? 145 : 45;
          const items = Array.from({ length: Math.min(args.limit, total - args.offset) }, (_, index) => {
            const ordinal = index + args.offset;
            if (args.kind === "concepts") return { concept_id: `concept_${ordinal}`, revision: 1, canonical_name: `${project === b ? "B" : "A"} concept ${ordinal}`, aliases: [], needs_full_explanation: true };
            if (args.kind === "atoms") return { atom_id: `atom_${ordinal}`, revision: 1, content: "Synthetic unverified statement", qualifiers: ["Only this synthetic setup"], evidence_links: [{ evidence_id: `evidence_${ordinal}`, relation: "supports" }], verification: "unverified", value_status: "known" };
            return { evidence_id: `evidence_${ordinal}`, source_id: "selected_A", source_sha256: "a".repeat(64), page_number: 3, page_sha256: "b".repeat(64), quote: "Original fixed quote", start_byte: 0, end_byte: 20 };
          });
          const result = { schema_version: 1, run_id: args.runId, kind: mock.wrongQuery ? "wrong" : args.kind, offset: args.offset, limit: args.limit, total, items };
          if (mock.deferQuery) { mock.deferQuery = false; return new Promise((resolve) => { mock.releaseQuery = () => resolve(result); }); }
          return result;
        }
        if (command === "compile_evidence") return { evidence: { evidence_id: args.evidenceId, source_id: "selected_A", page_number: 3, page_sha256: "b".repeat(64), quote: "Original fixed quote" }, page: { source_id: "selected_A", page_number: 3, status: "needs_review", text: "Original page before subsequent OCR", regions: [], issues: [], geometry: {}, width_pt: 600, height_pt: 800 } };
        if (command === "compile_export") return { status: "pending", output_root: `${args.parent}\\${args.name}`, counts: { concepts: 45, atoms: 45, evidence: 45 } };
        if (command === "task_kinds") return [];
        if (command === "get_close_behavior") return "hide";
        if (command === "pick_directory") return "C:\\synthetic\\exports";
        if (command === "plugin:window|is_maximized") return false;
        if (command === "plugin:event|listen") return 1;
        if (command === "plugin:event|unlisten") return null;
        throw new Error(`Unhandled mock IPC: ${command}`);
      },
    };
  });
  await page.goto("http://127.0.0.1:1421/");
  const nav = (name) => page.locator(".activity-rail").getByRole("button", { name, exact: true }).click();
  const calls = () => page.evaluate(() => window.__compileMock.calls);
  await check("Selected source creates all 3648 physical pages without strategies or an HTTP request", async () => {
    await nav("资料"); await page.getByRole("button", { name: "整理整本资料（全部 3,648 页）" }).click();
    await page.getByRole("heading", { name: "查询未核验知识" }).waitFor();
    const creation = (await calls()).filter((call) => call.command === "compile_create");
    assert(creation.length === 1, "create once"); assert(JSON.stringify(creation[0].args.sourceIds) === '["selected_A"]', "only explicitly selected PDF");
    assert(Object.keys(creation[0].args).sort().join() === "project,sourceIds", "no limits/model/config in UI IPC");
    assert(!(await calls()).some((call) => call.command === "compile_run" || call.command === "source_extract"), "create does not send requests");
    await page.getByText("3,648 页", { exact: true }).last().waitFor();
  });
  await check("Long worker survives navigation; pause uses durable compile control and resume preserves run identity", async () => {
    await page.getByRole("button", { name: "开始整理", exact: true }).click();
    await page.getByRole("button", { name: "正在自动分批整理…" }).waitFor();
    await nav("资料"); await nav("任务中心");
    await page.getByRole("button", { name: "暂停整理", exact: true }).click();
    await page.getByRole("button", { name: "恢复整理", exact: true }).waitFor();
    await page.getByRole("button", { name: "恢复整理", exact: true }).click();
    await page.getByRole("button", { name: "暂停整理", exact: true }).click();
    await page.getByRole("button", { name: "恢复整理", exact: true }).waitFor();
    const log = await calls(); assert(log.filter((call) => call.command === "compile_run").length === 2, "two explicit worker starts");
    assert(log.filter((call) => call.command === "compile_control").every((call) => call.args.action === "pause" && call.args.runId === "run_A"), "pause keeps run");
    assert(!log.some((call) => call.command === "source_cancel"), "no legacy cancellation");
  });
  await check("Retained error blocks ordinary start including paused status; explicit retry queues without HTTP or refund before manual resume", async () => {
    const before = await page.evaluate(() => JSON.parse(localStorage.getItem("noetome.mock-compile-runs"))["C:\\synthetic\\project-a"]);
    const starts = (await calls()).filter((call) => call.command === "compile_run").length;
    await page.evaluate(() => window.__compileMock.failRun());
    await page.getByRole("button", { name: "刷新任务", exact: true }).click();
    await page.getByRole("alert").filter({ hasText: "EXTRACT_TIMEOUT" }).waitFor();
    assert(await page.getByRole("button", { name: "恢复整理", exact: true }).isDisabled(), "failed run cannot start directly");
    // Simulate a control issued outside this screen and a summary that changes
    // status to paused while retaining the same durable failure. The UI gate
    // must depend on the error even when the backend also keeps failures sticky.
    await page.evaluate(async () => window.__TAURI_INTERNALS__.invoke("compile_control", { project: "C:\\synthetic\\project-a", runId: "run_A", action: "pause" }));
    await page.getByRole("button", { name: "刷新任务", exact: true }).click();
    await page.locator("#compile-progress-title").filter({ hasText: "已暂停" }).waitFor();
    await page.getByRole("alert").filter({ hasText: "EXTRACT_TIMEOUT" }).waitFor();
    assert(await page.getByRole("button", { name: "恢复整理", exact: true }).isDisabled(), "paused status cannot bypass a retained error");
    await page.getByText("任务仍保留错误，尚不能继续整理。", { exact: false }).waitFor();
    await page.getByRole("button", { name: "重试失败片段", exact: true }).click();
    await page.getByRole("status").filter({ hasText: "可重试的失败片段已重新排队" }).waitFor();
    await page.locator("#compile-progress-title").filter({ hasText: "等待开始" }).waitFor();
    const queued = await page.evaluate(() => JSON.parse(localStorage.getItem("noetome.mock-compile-runs"))["C:\\synthetic\\project-a"]);
    assert(queued.status === "queued" && queued.error === null, "retry cleared the stop and queued the same run");
    assert(queued.run_id === before.run_id && queued.units_confirmed === before.units_confirmed && queued.attempts === before.attempts, "identity/confirmed output and attempts preserved");
    assert(JSON.stringify(queued.budget) === JSON.stringify(before.budget), "unknown reservation was not refunded");
    assert(await page.getByText("9,000", { exact: true }).count() === 1, "unknown budget remains visible");
    const afterRetry = await calls();
    assert(afterRetry.filter((call) => call.command === "compile_run").length === starts, "retry sends no automatic HTTP");
    assert(afterRetry.filter((call) => call.command === "compile_control").at(-1).args.action === "retry", "explicit durable retry IPC");
    assert(await page.getByRole("button", { name: "恢复整理", exact: true }).isEnabled(), "manual resume is reachable");
    await page.getByRole("button", { name: "恢复整理", exact: true }).click();
    await page.getByRole("button", { name: "暂停整理", exact: true }).click();
    await page.getByRole("button", { name: "恢复整理", exact: true }).waitFor();
    assert((await calls()).filter((call) => call.command === "compile_run").length === starts + 1, "one explicit manual worker start after retry");
  });
  await check("Concept pagination reaches beyond the old unit sample; search resets offset", async () => {
    await page.getByRole("heading", { name: "A concept 0", exact: true }).waitFor();
    for (let offset = 20; offset <= 80; offset += 20) {
      await page.getByRole("button", { name: "下一页结果" }).click(); await page.getByRole("heading", { name: `A concept ${offset}`, exact: true }).waitFor();
    }
    await page.getByLabel("搜索当前类型").fill("Synthetic"); await page.getByRole("button", { name: "查询", exact: true }).click();
    await page.getByRole("heading", { name: "A concept 0", exact: true }).waitFor();
    const query = (await calls()).filter((call) => call.command === "compile_query").at(-1);
    assert(query.args.offset === 0 && query.args.limit === 20 && query.args.search === "Synthetic", "search/pager ranges");
  });
  await check("Atom-to-evidence jump shows original historical page and never substitutes current corpus pages", async () => {
    await page.getByRole("button", { name: "知识原子", exact: true }).click();
    await page.getByRole("button", { name: "查看来源证据 evidence_0", exact: true }).click();
    await page.getByRole("button", { name: "查看本页原文", exact: true }).click();
    await page.getByText("Original page before subsequent OCR", { exact: true }).waitFor();
    const evidence = (await calls()).filter((call) => call.command === "compile_evidence").at(-1);
    assert(evidence.args.runId === "run_A" && evidence.args.evidenceId === "evidence_0", "exact saved evidence");
    assert(await page.getByText("所有生成知识都等待独立核验。", { exact: false }).count() === 1, "unverified label");
  });
  await check("Malformed query binding is visible; recovering query leaves product expectations unchanged", async () => {
    await page.evaluate(() => { window.__compileMock.wrongQuery = true; });
    await page.getByRole("button", { name: "来源证据", exact: true }).click();
    await page.getByRole("alert").filter({ hasText: "DESKTOP_COMPILE_BINDING" }).waitFor();
    await page.evaluate(() => { window.__compileMock.wrongQuery = false; });
    await page.getByRole("button", { name: "概念", exact: true }).click(); await page.getByRole("heading", { name: "A concept 0", exact: true }).waitFor();
  });
  await check("Export passes the fixed run and new destination; pending budget stays visible", async () => {
    await page.getByLabel("导出父目录", { exact: true }).fill("C:\\synthetic\\exports");
    await page.getByLabel("新文件夹名", { exact: true }).fill("pending-notes"); await page.getByRole("button", { name: "导出到新目录", exact: true }).click();
    await page.getByRole("status").filter({ hasText: "未核验笔记已导出" }).waitFor();
    const result = (await calls()).filter((call) => call.command === "compile_export").at(-1);
    assert(result.args.runId === "run_A" && result.args.name === "pending-notes", "new fixed-run export");
    assert(await page.getByText("待核算预留 Token", { exact: true }).count() === 1, "unknown usage visible");
  });
  await check("Reopened project restores paused run without creating a new task or worker", async () => {
    await page.reload(); await nav("任务中心"); await page.getByRole("button", { name: "恢复整理", exact: true }).waitFor();
    const log = await calls(); assert(!log.some((call) => ["compile_create", "compile_run"].includes(call.command)), "read-only restore");
  });
  await check("Out-of-order response from the old project cannot populate the new project's knowledge", async () => {
    await page.evaluate(() => { window.__compileMock.deferQuery = true; });
    await page.getByRole("button", { name: "下一页结果" }).click();
    await page.waitForFunction(() => typeof window.__compileMock.releaseQuery === "function");
    await nav("项目"); await page.getByRole("textbox", { name: "项目目录", exact: true }).fill("C:\\synthetic\\project-b"); await page.getByRole("button", { name: "打开项目", exact: true }).click();
    await nav("任务中心"); await page.getByRole("checkbox").first().check(); await page.getByRole("button", { name: "建立整书任务", exact: true }).click();
    await page.getByRole("heading", { name: "B concept 0", exact: true }).waitFor();
    await page.evaluate(() => window.__compileMock.releaseQuery()); await page.waitForTimeout(100);
    assert(await page.getByRole("heading", { name: "A concept 20", exact: true }).count() === 0, "stale project hidden");
  });
  assert(failures.length === 0, `Renderer errors: ${failures.join("; ")}`);
  return { schema_version: 1, mode: "browser_IPC_mock_zero_model_requests", passed: checks.length, failed: 0, checks, renderer_errors: failures };
}
