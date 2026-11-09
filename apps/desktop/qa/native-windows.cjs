/* Actual Windows Tauri/WebView2 CDP QA. No mock IPC or browser launch. */
"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const http = require("node:http");
const { spawn } = require("node:child_process");

const manifest = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const { chromium } = require(manifest.playwright_module);
const evidence = manifest.evidence_root;
fs.mkdirSync(evidence, { recursive: true });
const checks = [], commands = [], ipc = [], pageErrors = [], consoleErrors = [];
let browser, page, fakeServer, fakeRequests = 0;
const expectedCapabilities = ["help", "version", "project.create", "project.open", "task.read", "source.pick", "project.create_in_parent", "source.import", "source.choices", "source.units", "source.extract", "source.preview_export", "model.status", "model.credential_store", "model.budget_restore", "desktop.self_test", "source.cancel"];
const sha = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
const save = (name, value) => fs.writeFileSync(path.join(evidence, name), JSON.stringify(value, null, 2) + "\n");
const fingerprint = file => { const bytes = fs.readFileSync(file); return { sha256: sha(bytes), byte_length: bytes.length }; };
async function check(name, action) {
  try { const detail = await action(); checks.push({ name, passed: true, detail: detail ?? null }); }
  catch (error) { checks.push({ name, passed: false, error: String(error.stack || error) }); throw error; }
}
async function waitFor(test, description, timeout = 45000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (await test()) return; await new Promise(r => setTimeout(r, 100)); }
  throw new Error(`Timed out: ${description}`);
}
async function native(command, args = {}) {
  assert(["runtime_info", "project_open", "source_choices", "source_units", "model_status", "get_close_behavior"].includes(command), `IPC must be read-only: ${command}`);
  const value = await page.evaluate(async ({ command, args }) => {
    if (typeof window.__TAURI_INTERNALS__?.invoke !== "function") throw new Error("Actual Tauri IPC missing");
    return window.__TAURI_INTERNALS__.invoke(command, args);
  }, { command, args });
  ipc.push({ command, args, value }); save("actual-readonly-ipc.json", ipc); return value;
}
const main = () => page.locator("#main-content");
async function navigate(name) { await page.locator(".activity-rail").getByRole("button", { name, exact: true }).click(); }
async function screenshot(name) { await page.screenshot({ path: path.join(evidence, name + ".png") }); }
function snapshot(root) {
  const files = {};
  function walk(current) {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const item = path.join(current, entry.name);
      assert(!entry.isSymbolicLink(), "Unexpected link inside owned QA root");
      if (entry.isDirectory()) walk(item); else if (entry.isFile()) files[path.relative(root, item)] = fingerprint(item);
    }
  }
  walk(root); return files;
}
async function cli(name, args, env = {}) {
  const started = new Date().toISOString();
  const proc = spawn(manifest.cli_exe, args, { cwd: manifest.run_root, env: { ...process.env, ...env }, windowsHide: true });
  const stdout = [], stderr = [];
  proc.stdout.on("data", data => stdout.push(data)); proc.stderr.on("data", data => stderr.push(data));
  let timer;
  const exit = await new Promise((resolve, reject) => {
    timer = setTimeout(() => { proc.kill(); reject(new Error("Installed CLI timeout")); }, 45000);
    proc.once("error", reject); proc.once("close", (code, signal) => resolve({ code, signal }));
  }).finally(() => clearTimeout(timer));
  const out = Buffer.concat(stdout), err = Buffer.concat(stderr);
  fs.writeFileSync(path.join(evidence, name + ".stdout.bin"), out); fs.writeFileSync(path.join(evidence, name + ".stderr.bin"), err);
  commands.push({ name, command: [manifest.cli_exe, ...args], started, finished: new Date().toISOString(), ...exit }); save("actual-cli-commands.json", commands);
  assert.equal(exit.code, 0, err.toString()); return out;
}
async function localFakeApi(project, importId, selection) {
  const apiRoot = path.join(manifest.run_root, "作者 HTTP 假服务结果");
  fakeServer = http.createServer(async (req, res) => {
    try {
      assert.equal(req.method, "POST"); assert.equal(req.url, "/v1/responses");
      assert.equal(req.headers.authorization, "Bearer noetome-test-public-credential-never-real");
      const chunks = []; for await (const chunk of req) chunks.push(chunk);
      const raw = Buffer.concat(chunks); fs.writeFileSync(path.join(evidence, "local-fake-request.bin"), raw);
      const request = JSON.parse(JSON.parse(raw)["input"]); const unit = request.units[0];
      const candidate = { schema_version: 1, request_id: request.request_id, binding: request.binding, concepts: [{ concept_id: "qa-concept", revision: 1, canonical_name: "本地 HTTP 假服务候选\n仅用于安装测试", aliases: [], needs_full_explanation: true }], evidence: [{ evidence_id: "qa-evidence", unit_id: unit.unit_id, excerpt_start_byte: unit.start_byte, excerpt_end_byte: unit.end_byte, excerpt_sha256: sha(Buffer.from(unit.text)), quote: unit.text }], atoms: [{ atom: { atom_id: "qa-atom", revision: 1, kind: "comparison_value", concept_ids: ["qa-concept"], content: "作者测试候选，数值未知。 $2+2$ ~~strike~~ ==highlight==\n~~~\n模型原样文本", qualifiers: ["仅用于安装链路测试"], evidence_links: [{ evidence_id: "qa-evidence", relation: "supports" }], required: false, verification: "unverified", value_status: "unknown" }, measurement: { value_status: "unknown", value: null, unit: null, conditions: ["仅用于安装链路测试"] } }] };
      const response = Buffer.from(JSON.stringify({ id: "resp_native_qa", object: "response", model: "noetome-test-model", status: "completed", error: null, incomplete_details: null, service_tier: "default", output: [{ type: "message", role: "assistant", status: "completed", id: "msg_native_qa", content: [{ type: "output_text", text: JSON.stringify(candidate), annotations: [], logprobs: [] }] }], usage: { input_tokens: 100, input_tokens_details: { cached_tokens: 0, cache_write_tokens: 0 }, output_tokens: 600, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 700 } }));
      fs.writeFileSync(path.join(evidence, "local-fake-response.bin"), response); save("author-local-fake-candidate.json", candidate);
      fakeRequests++; res.writeHead(200, { "Content-Type": "application/json", "Content-Length": response.length }); res.end(response);
    } catch (error) { save("local-fake-server-error.json", { error: String(error.stack || error) }); res.writeHead(500); res.end("Author QA server failed"); }
  });
  await new Promise(resolve => fakeServer.listen(0, "127.0.0.1", resolve));
  const config = path.join(manifest.run_root, "local-http-fake-config.json");
  fs.writeFileSync(config, JSON.stringify({ schema_version: 1, provider: "openai_responses", transport_mode: "local_http_test", endpoint: `http://127.0.0.1:${fakeServer.address().port}/v1/responses`, model: "noetome-test-model", credential_env: "NOETOME_TEST_CREDENTIAL", required_input: "text", capabilities: { text: true, structured_output: true, vision: false }, selection, limits: { max_units: 64, max_input_bytes: 65536, max_response_bytes: 65536, max_output_tokens: 2048, timeout_ms: 10000 }, budget: { max_requests: 1, max_total_tokens: 150000, max_cost_usd: null } }, null, 2));
  await cli("installed-cli-local-fake-api", ["extract", "api", project, importId, config, apiRoot], { NOETOME_TEST_CREDENTIAL: "noetome-test-public-credential-never-real" });
  assert.equal(fakeRequests, 1); return apiRoot;
}

(async () => {
  try {
    await check("actual Windows x64 release payload and isolated profiles", async () => {
      assert.equal(process.platform, "win32"); assert.equal(process.arch, "x64"); assert.equal(manifest.schema_version, 1);
      assert(["local_staged_payload", "ci_clean_installed", "local_ci_installed_payload"].includes(manifest.mode));
      assert.equal(process.env.NOETOME_DESKTOP_DATA_ROOT, manifest.data_root);
      assert.equal(process.env.LOCALAPPDATA, manifest.local_app_data);
      assert.equal(fingerprint(manifest.desktop_exe).sha256, manifest.desktop_sha256);
      assert.equal(fingerprint(manifest.cli_exe).sha256, manifest.cli_sha256);
      return { mode: manifest.mode, source_commit: manifest.source_commit, desktop: fingerprint(manifest.desktop_exe), cli: fingerprint(manifest.cli_exe), data_root: manifest.data_root };
    });
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${manifest.port}`);
    await waitFor(async () => {
      for (const context of browser.contexts()) for (const candidate of context.pages()) {
        if (/^(https?:\/\/tauri\.localhost|tauri:)/.test(candidate.url()) && await candidate.evaluate(() => typeof window.__TAURI_INTERNALS__?.invoke === "function").catch(() => false)) { page = candidate; return true; }
      }
      return false;
    }, "actual embedded Tauri page");
    page.on("pageerror", error => pageErrors.push(String(error))); page.on("console", message => { if (message.type() === "error") consoleErrors.push(message.text()); });
    await check("embedded release UI and actual runtimeInfo17 capabilities", async () => {
      assert(!/localhost:1420|127\.0\.0\.1:1420/.test(page.url()));
      const actual = await native("runtime_info"); assert.deepEqual(actual.capabilities, expectedCapabilities); assert.equal(actual.target, "x86_64-pc-windows-msvc"); assert.equal(actual.app_version, actual.cli_version);
      await navigate("运行检查"); await main().getByRole("button", { name: "检查连接", exact: true }).click();
      await waitFor(async () => (await main().innerText()).includes("已收到本地 Rust 组件的实际返回结果"), "real runtime displayed");
      await main().getByRole("button", { name: "查看版本", exact: true }).click(); await waitFor(async () => (await page.getByLabel("CLI 运行结果").innerText()).includes(actual.cli_version), "CLI version displayed");
      await screenshot("native-runtime"); return { page_url: page.url(), ...actual };
    });
    const projectName = "作者 PDF 中文 实测项目", project = path.join(manifest.project_parent, projectName);
    await check("native project creation in CJK and space directory", async () => {
      await navigate("项目"); await page.locator("#project-path").fill(manifest.project_parent); await page.locator("#project-name").fill(projectName); await main().getByRole("button", { name: "新建项目", exact: true }).click();
      await waitFor(async () => await main().locator(".project-details").isVisible(), "project real fields");
      const actual = await native("project_open", { path: project }); assert.equal(actual.state, "initialized"); assert.equal(actual.schema_version, 1);
      assert.deepEqual(fs.readdirSync(project).sort(), ["project.json", "project.sqlite"]); assert((await page.locator("#sidebar-projects").innerText()).includes(actual.project_id));
      await screenshot("native-project"); return actual;
    });
    let importId, choices, units, selection;
    await check("actual author PDF import and full selected Region preflight", async () => {
      await navigate("资料"); await page.locator("#source-path").fill(manifest.selected_text_pdf); await main().getByRole("button", { name: "导入并检查", exact: true }).click();
      await waitFor(async () => await page.locator("#source-choice-title").isVisible(), "actual PDF choices", 60000);
      importId = await page.locator("#source-import-id").inputValue(); assert.match(importId, /^[a-f0-9-]{36}$/);
      choices = await native("source_choices", { project, importId }); assert.notEqual(choices.source_status, "failed"); assert(choices.choices.some(c => c.selectable && c.byte_length > 0));
      const chosen = choices.choices.find(c => c.selectable && c.byte_length > 0); selection = [chosen.selection]; assert.equal(chosen.selection.kind, "pdf");
      const index = choices.choices.indexOf(chosen); await page.locator(".source-choice input").nth(index).check(); await main().getByRole("button", { name: "查看将发送的完整原文", exact: true }).click();
      await waitFor(async () => await page.locator(".source-unit > pre").count() > 0, "preflight original text");
      units = await native("source_units", { project, importId, selection }); assert.deepEqual(units.binding, choices.binding); assert.equal(units.units.length, 1); assert.equal(await page.locator(".source-unit > pre").first().textContent(), units.units[0].text);
      assert((await page.locator(".source-unit").first().innerText()).includes(`物理第 ${units.units[0].locator.page_index + 1} 页`)); await screenshot("native-pdf-preflight"); return { import_id: importId, choices, units };
    });
    await check("page switching preserves selection and consent while missing model blocks paid action", async () => {
      await main().getByLabel("我已核对以上原文，同意将这些文字发送给当前模型服务。").check(); await navigate("运行检查"); await navigate("资料");
      assert.equal(await page.locator(".source-unit > pre").first().textContent(), units.units[0].text); assert(await main().getByLabel("我已核对以上原文，同意将这些文字发送给当前模型服务。").isChecked());
      const status = await native("model_status"); assert.equal(status.ready, false); assert.equal(status.credential_state, "missing"); assert.equal(status.ledger_state, "missing");
      assert.equal(status.accounted_tokens, null); assert(await main().getByRole("button", { name: "生成候选（调用模型）", exact: true }).isDisabled());
      await main().getByRole("button", { name: "前往模型设置", exact: true }).click(); await waitFor(async () => (await main().innerText()).includes("累计账本缺失"), "missing ledger UI");
      assert(await page.locator("#app-sidebar").isHidden()); assert(await main().getByRole("button", { name: "按历史快照恢复账本", exact: true }).isDisabled()); assert(!(await main().locator(".budget-restore input").isChecked()));
      await screenshot("native-model-missing"); assert(!fs.existsSync(path.join(manifest.data_root, "model-profile/credentials/tangmao-deepseek.dpapi"))); assert(!fs.existsSync(path.join(manifest.data_root, "model-profile/model-services/tangmao-deepseek.usage.json"))); return status;
    });
    let apiRoot;
    await check("installed CLI sends one explicit author local HTTP fake request", async () => { apiRoot = await localFakeApi(project, importId, selection); return { api_root: apiRoot, local_fake_requests: fakeRequests, live_paid_calls: 0 }; });
    const outputName = "作者 待核验笔记", output = path.join(manifest.output_parent, outputName);
    await check("GUI recovers actual API result into a new pending Vault with zero extra HTTP", async () => {
      const sourceBefore = snapshot(project), apiBefore = snapshot(apiRoot), callsBefore = fakeRequests;
      await navigate("资料"); await page.locator("#result-root").fill(apiRoot); await page.locator("#export-parent").fill(manifest.output_parent); await page.locator("#export-name").fill(outputName); await main().getByRole("button", { name: "导出待核验笔记", exact: true }).click();
      await waitFor(async () => (await main().innerText()).includes("待核验笔记已导出"), "actual GUI pending export");
      assert.equal(fakeRequests, callsBefore); assert.deepEqual(snapshot(project), sourceBefore); assert.deepEqual(snapshot(apiRoot), apiBefore);
      const preview = JSON.parse(fs.readFileSync(path.join(output, "reports/preview.json"))); assert.equal(preview.producer, "extraction_preview"); assert.equal(preview.published, false); assert.equal(preview.scientific_quality, "not_run"); assert.deepEqual(preview.units, units.units);
      assert.deepEqual(fs.readFileSync(path.join(output, "vault/attachments/source-0001.pdf")), fs.readFileSync(manifest.selected_text_pdf));
      const concept = fs.readFileSync(path.join(output, "vault/concept-0001.md"), "utf8"), ev = fs.readFileSync(path.join(output, "vault/evidence-0001.md"), "utf8");
      assert(concept.includes("本地 HTTP 假服务")); assert(concept.includes("数值：未知 / 不适用（null）")); assert(concept.includes("仅用于安装链路测试")); assert(!concept.includes("\n~~~\n")); assert(ev.includes(`#page=${units.units[0].locator.page_index + 1}`));
      await screenshot("native-pending-vault"); save("native-generated-files.json", snapshot(output)); return { output_root: output, vault_path: path.join(output, "vault"), counts: preview.counts, extra_http_requests: fakeRequests - callsBefore };
    });
    await check("existing pending output is rejected without modification", async () => {
      const before = snapshot(output); await main().getByRole("button", { name: "导出待核验笔记", exact: true }).click();
      await waitFor(async () => (await main().innerText()).includes("EXTRACT_OUTPUT_EXISTS"), "existing output refusal"); assert.deepEqual(snapshot(output), before); assert.equal(fakeRequests, 1); await screenshot("native-export-refused");
    });
    await check("actual scan PDF remains needs_review with no selectable fake text", async () => {
      await page.locator("#source-path").fill(manifest.selected_scan_pdf); await main().getByRole("button", { name: "导入并检查", exact: true }).click();
      await waitFor(async () => await page.locator("#source-choice-title").isVisible() || (await main().innerText()).includes("资料导入失败，未进入模型步骤"), "scan PDF completed report", 60000);
      let scanId = await page.locator("#source-import-id").inputValue(), record = null;
      const nonzeroIncomplete = (await main().innerText()).includes("资料导入失败，未进入模型步骤");
      if (nonzeroIncomplete) {
        const summary = main().getByText("完整本地记录", { exact: true }).first(); await summary.click();
        record = JSON.parse(await summary.locator("..").locator("pre").textContent()); scanId = record.import_id;
        assert.equal(record.pdf?.status || record.status, "needs_review"); assert((await main().innerText()).includes("PDF_INCOMPLETE"));
        assert.equal(await main().getByRole("button", { name: "生成候选（调用模型）", exact: true }).count(), 0);
      } else assert((await main().innerText()).includes("没有可用于模型整理的文字区域"));
      const actual = await native("source_choices", { project, importId: scanId }); assert.equal(actual.source_status, "needs_review"); assert(!actual.choices.some(c => c.selectable && c.byte_length > 0));
      assert.equal(await page.locator(".source-unit").count(), 0); assert.equal(fakeRequests, 1); await screenshot("native-scan-needs-review"); return { nonzero_pdf_incomplete_retained: nonzeroIncomplete, import_record: record, choices: actual };
    });
    await check("payload offline author fixture self-test stays separate from own PDF", async () => {
      await navigate("工作台"); await main().getByRole("button", { name: "运行离线样例", exact: true }).click();
      await waitFor(async () => (await main().innerText()).includes("离线作者样例自检完成"), "actual payload selftest", 60000);
      await main().getByText("完整离线记录", { exact: true }).click(); const record = JSON.parse(await main().locator(".workflow-record pre").textContent());
      assert.equal(record.desktop_self_test.kind, "fixture_authored_offline"); assert.equal(record.desktop_self_test.model_calls, 0); assert.equal(record.desktop_self_test.network_requests, 0); assert.equal(fakeRequests, 1); await screenshot("native-offline-selftest"); return record;
    });
    await check("actual Tauri ownership marker and isolated window preferences", async () => {
      const owner = JSON.parse(fs.readFileSync(path.join(manifest.data_root, ".noetome-desktop-data.json")));
      assert.equal(owner.producer, "noetome_desktop"); assert.equal(owner.purpose, "isolated_application_data");
      assert.equal(owner.root.replace(/^\\\\\?\\/, "").toLowerCase(), manifest.data_root.toLowerCase());
      assert.equal(await native("get_close_behavior"), "hide_to_tray");
      assert(!fs.existsSync(path.join(manifest.data_root, "logs/startup-error.log")));
      return { owner, close_behavior: "hide_to_tray", requested_data_root: manifest.data_root };
    });
    await check("native light and dark screenshots, no page or console errors", async () => {
      await navigate("资料"); await page.locator(".activity-rail").getByRole("button", { name: "切换到暗色", exact: true }).click(); assert.equal(await page.locator("html").getAttribute("data-theme"), "dark"); await screenshot("native-scan-dark");
      await page.locator(".activity-rail").getByRole("button", { name: "切换到亮色", exact: true }).click(); await screenshot("native-scan-light"); assert.deepEqual(pageErrors, []); assert.deepEqual(consoleErrors, []);
      return await page.evaluate(() => ({ innerWidth, innerHeight, devicePixelRatio, userAgent: navigator.userAgent, platform: navigator.platform }));
    });
  } catch (error) { save("fatal-error.json", { error: String(error.stack || error) }); if (page) await screenshot("native-failure").catch(() => {}); process.exitCode = 1; }
  finally {
    if (fakeServer) await new Promise(resolve => fakeServer.close(resolve));
    save("native-ui-result.json", { schema_version: 1, scope: "actual_native_windows_tauri_ui", mode: manifest.mode, delivery_installer_sha256: manifest.delivery_installer_sha256 ?? null, installed_payload_zip_sha256: manifest.installed_payload_zip_sha256 ?? null, passed: checks.length === 12 && checks.every(c => c.passed) && !process.exitCode, checks, page_errors: pageErrors, console_errors: consoleErrors, local_fake_http_requests: fakeRequests, live_paid_calls: 0, mocked_ipc: false, installer_actions_local: 0, original_user_profile_access_requested: false, native_filepicker: "not_run", actual_obsidian_reading_view: "not_run" });
    if (browser) await browser.close().catch(() => {});
  }
})().catch(error => { save("harness-error.json", { error: String(error.stack || error) }); process.exitCode = 1; });
