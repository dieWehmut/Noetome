async (page) => {
  const evidence = '.artifacts/B15-T02/frontend';
  const checks = [];
  const consoleErrors = [];
  page.on('pageerror', error => consoleErrors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') consoleErrors.push(message.text()); });
  const assert = (condition, name) => { checks.push({ name, passed: !!condition }); if (!condition) throw new Error(name); };
  const rail = () => page.getByRole('navigation', { name: '快捷导航' });
  const navigate = async name => rail().getByRole('button', { name, exact: true }).click();
  const sentText = '原文完整证据：在 37 °C 条件下，样品浓度为 2.5 μmol/L。\n第二行也会原样显示，不省略、不补写。';
  async function setup(scenario = 'success') {
    await page.goto('http://127.0.0.1:1420');
    await page.getByRole('heading', { level: 1 }).waitFor();
    await page.evaluate(async ({ scenario, sentText }) => {
      const { mockIPC, mockWindows } = await import('/qa/tauriMocks.ts');
      mockWindows('main'); globalThis.isTauri = true;
      const q = window.qa = { scenario, calls: [], credential: scenario !== 'missing', restored: scenario !== 'missing', sentText };
      const status = () => ({ ready: q.credential && q.restored && q.scenario !== 'invalid-ledger', endpoint: 'https://qa.example.invalid/v1/responses', model: 'QA synthetic model', credential_state: q.credential ? 'ready' : 'missing', ledger_state: q.scenario === 'invalid-ledger' ? 'invalid' : q.restored ? 'ready' : 'missing', accounted_tokens: q.restored ? 33185 : null, pending_reserved_tokens: q.restored ? 0 : null, remaining_tokens: q.restored ? 9966815 : null, model_attempts: q.restored ? 10 : null, message: q.credential && q.restored ? 'QA mock 模型就绪' : 'QA mock 凭据或累计账本缺失' });
      mockIPC(async (cmd, args = {}) => {
        q.calls.push({ command: cmd, args });
        const project = path => ({ path, project_id: path.includes('项目二') ? 'project-2' : 'project-1', schema_version: 3, created_at: '2026-11-09T00:00:00Z', state: 'initialized', database: 'project.sqlite' });
        const binding = (args) => ({ project_id: args.project.includes('项目二') ? 'project-2' : 'project-1', import_id: args.importId, source_id: 'qa-source', version_id: 'qa-version', content_sha256: 'a'.repeat(64), normalized_sha256: null, parse_id: 'qa-parse', manifest_sha256: 'b'.repeat(64) });
        const extracted = (args) => ({ record: { binding: { ...binding(args), ...(q.scenario === 'bad-extract-binding' ? { version_id: 'other-version' } : {}) }, status: 'candidates_validated', knowledge_status: 'pending', scientific_status: 'not_run', published: false, candidate_count: 1 }, failure: null, output_root: 'D:\\QA outputs\\API 候选' });
        if (cmd === 'model_status') return status();
        if (cmd === 'model_credential_store') { q.credential = true; return status(); }
        if (cmd === 'model_budget_restore') { if (args.confirmed !== true || q.restored) throw 'QA unexpected budget restore'; q.restored = true; return status(); }
        if (cmd === 'project_create_in_parent') return project(args.parent + '\\' + args.name);
        if (cmd === 'project_open') return project(args.path);
        if (cmd === 'pick_directory') return 'D:\\QA 中文目录 with spaces';
        if (cmd === 'source_pick') return 'D:\\QA 资料\\自己的 PDF.pdf';
        if (cmd === 'source_import') return { project: project(args.project), record: { import_id: args.project.includes('项目二') ? 'import-2' : 'import-1', status: q.scenario === 'failed-import' ? 'failed' : 'ready', knowledge_status: 'pending', selected_path: args.path }, failure: q.scenario === 'failed-import' ? { code: 'PDF_PARSE_FAILED', message: 'QA synthetic PDF parser failure' } : null };
        if (cmd === 'source_choices' && (q.scenario === 'many' || q.scenario === 'bulk-bytes')) return { schema_version: 1, binding: binding(args), filename: 'QA many regions.pdf', source_status: 'ready', choices: Array.from({ length: 70 }, (_, index) => ({ selection: { kind: 'pdf', page_index: 0, region_id: 'region-' + index }, label: '原文区域 ' + index, byte_length: q.scenario === 'bulk-bytes' ? index < 2 ? 0 : 40000 : 1, source_status: 'ready', selectable: index !== 0 })) };
        if (cmd === 'source_choices') return { schema_version: 1, binding: { ...binding(args), ...(q.scenario === 'bad-choice-binding' ? { project_id: 'other-project' } : {}) }, filename: '自己的 PDF.pdf', source_status: q.scenario === 'needs-review' ? 'needs_review' : 'ready', choices: q.scenario === 'no-text' ? [] : [{ selection: q.scenario === 'text' ? { kind: 'text', ordinal: 0 } : { kind: 'pdf', page_index: 0, region_id: 'region-1' }, label: q.scenario === 'needs-review' ? '第1页：扫描区域待复核' : '第1页：已确认原文', byte_length: q.scenario === 'oversize' ? 65537 : new TextEncoder().encode(sentText).length, source_status: q.scenario === 'needs-review' ? 'needs_review' : 'ready', selectable: q.scenario !== 'needs-review', reason: q.scenario === 'needs-review' ? '没有确认的文字' : null }] };
        if (cmd === 'source_units') return { binding: { ...binding(args), ...(q.scenario === 'bad-unit-binding' ? { version_id: 'other-version' } : {}) }, units: [{ unit_id: 'unit-1', locator: args.selection[0].kind === 'text' ? { kind: 'text', ordinal: 0, span_id: 'qa-span', resource_sha256: 'c'.repeat(64) } : { kind: 'pdf', page_index: 0, region_id: 'region-1', parse_id: 'qa-parse', resource_id: 'qa-resource', resource_sha256: 'c'.repeat(64), bbox: [0, 0, 100, 200] }, source_status: 'ready', start_byte: 0, end_byte: new TextEncoder().encode(sentText).length, text: sentText }] };
        if (cmd === 'source_extract') {
          if (q.scenario === 'late' || q.scenario === 'cancel') return new Promise(resolve => { q.release = () => resolve(q.scenario === 'cancel' ? { record: { status: 'failed' }, failure: { code: 'EXTRACT_CANCELLED', message: 'QA cancelled' }, output_root: 'D:\\QA outputs\\cancelled' } : extracted(args)); });
          if (q.scenario === 'failed-extract') return { record: { status: 'failed', knowledge_status: 'pending' }, failure: { code: 'EXTRACT_API_CANDIDATES', message: 'QA candidate validation failed' }, output_root: 'D:\\QA outputs\\failed' };
          return extracted(args);
        }
        if (cmd === 'source_cancel') { if (q.scenario === 'cancel') { q.release(); return true; } return false; }
        if (cmd === 'source_preview_export') {
          if (args.name === '已有 Vault') throw 'PREVIEW_UNSAFE_OUTPUT：QA existing or production Vault refused';
          return { producer: 'extraction_preview', status: 'pending', output_root: args.parent + '\\' + args.name, vault_path: args.parent + '\\' + args.name + '\\vault', counts: { concepts: 1, atoms: 1, evidence: 1 }, markdown: '# 待核验预览\n\nQA synthetic evidence; knowledge pending, scientific not_run.' };
        }
        if (cmd === 'desktop_self_test') return { record: { producer: 'fixture_author', model_calls: 0, network_requests: 0, knowledge_status: 'pending', scientific_status: 'not_run' }, failure: null, output_root: 'D:\\QA offline fixture', vault_path: 'D:\\QA offline fixture\\vault' };
        if (cmd === 'get_close_behavior') return 'hide_to_tray';
        if (cmd === 'runtime_info') return { app_version: 'QA', cli_version: 'QA', target: 'QA browser mock', capabilities: [], cli_help: 'QA' };
        if (cmd.startsWith('plugin:')) return false;
        throw 'QA unsupported command: ' + cmd;
      });
    }, { scenario, sentText });
  }
  async function modelPage() { await navigate('设置'); await page.getByRole('button', { name: '模型与预算', exact: true }).click(); await page.getByRole('button', { name: '刷新模型状态' }).click(); }
  async function makeProject(name = '项目一 中文 spaces', create = false) {
    await navigate('项目');
    if (create) { await page.locator('#project-path').fill('D:\\QA parent with spaces'); await page.locator('#project-name').fill(name); await page.getByRole('button', { name: '新建项目', exact: true }).click(); }
    else { await page.getByRole('button', { name: '打开已有项目' }).click(); await page.locator('#project-path').fill('D:\\QA parent with spaces\\' + name); await page.getByRole('button', { name: '打开项目', exact: true }).click(); }
    await page.getByRole('heading', { name: '当前项目', exact: true }).waitFor();
    await page.getByRole('button', { name: '前往资料' }).click();
  }
  async function importAndSelect() {
    await page.getByRole('button', { name: '选择资料文件' }).click();
    await page.getByRole('button', { name: '导入并检查' }).click();
    await page.getByRole('heading', { name: '2. 选择要整理的原文' }).waitFor();
    await page.locator('.source-choice input').check();
    await page.getByRole('button', { name: '查看将发送的完整原文' }).click();
    await page.locator('.source-unit > pre').waitFor();
    await page.getByRole('checkbox', { name: '我已核对以上原文，同意将这些文字发送给当前模型服务。' }).check();
  }
  async function calls(command) { return page.evaluate(command => window.qa.calls.filter(call => call.command === command), command); }
  try {
    await page.setViewportSize({ width: 1200, height: 900 });
    await setup();
    assert(await page.title() === 'Noetome' && page.url().startsWith('http://127.0.0.1:1420/'), 'page identity');
    assert(await page.getByRole('heading', { level: 1 }).count() === 1 && await page.locator('vite-error-overlay').count() === 0, 'meaningful screen without overlay');
    await modelPage();
    assert(await page.locator('#app-sidebar').isHidden(), 'model settings hide main sidebar');
    assert(await page.locator('input[type=password]').count() === 0, 'no React credential input');
    await makeProject('项目一 中文 spaces', true);
    assert((await calls('project_create_in_parent'))[0].args.name === '项目一 中文 spaces', 'Chinese and spaced project creation DTO');
    await importAndSelect();
    assert(await page.locator('.source-unit > pre').innerText() === sentText, 'complete exact selected source text displayed');
    await navigate('工作台'); await navigate('资料');
    assert(await page.locator('.source-unit > pre').innerText() === sentText && await page.getByRole('checkbox', { name: '我已核对以上原文，同意将这些文字发送给当前模型服务。' }).isChecked(), 'source and consent survive page changes');
    await page.getByRole('button', { name: '生成候选（调用模型）', exact: true }).click();
    await page.getByRole('heading', { name: '候选已生成 · 待核验' }).waitFor();
    assert((await calls('source_extract')).length === 1, 'one explicit model invocation');
    assert(JSON.stringify((await calls('source_extract'))[0].args.selection) === JSON.stringify([{ kind: 'pdf', page_index: 0, region_id: 'region-1' }]), 'selection passed verbatim with camelCase importId');
    assert((await calls('source_extract'))[0].args.confirmedUnits.units[0].text === sentText && (await calls('source_extract'))[0].args.confirmedUnits.binding.import_id === 'import-1', 'confirmedUnits carries exact displayed text and binding');
    assert(await page.getByText('物理第 1 页 · 区域 region-1 · 原文范围：', { exact: false }).count() === 1, 'PDF locator is human-readable');
    await page.locator('#export-parent').fill('D:\\QA outputs 中文 spaces');
    await page.locator('#export-name').fill('待核验 中文 notes');
    await page.getByRole('button', { name: '导出待核验笔记' }).click();
    await page.getByRole('heading', { name: '待核验笔记已导出' }).waitFor();
    assert(await page.locator('.preview-result').innerText().then(text => text.includes('extraction_preview · pending')), 'preview explicitly pending');
    await page.locator('#export-name').fill('已有 Vault'); await page.getByRole('button', { name: '导出待核验笔记' }).click();
    await page.getByRole('alert').filter({ hasText: 'PREVIEW_UNSAFE_OUTPUT' }).waitFor();
    assert(await page.getByRole('heading', { name: '待核验笔记已导出' }).count() === 0, 'invalid export has no success result');
    await page.locator('#result-root').fill('D:\\QA outputs\\saved result'); await page.locator('#export-name').fill('恢复笔记');
    await page.getByRole('button', { name: '导出待核验笔记' }).click(); await page.getByRole('heading', { name: '待核验笔记已导出' }).waitFor();
    assert((await calls('source_extract')).length === 1 && (await calls('source_preview_export')).length === 3, 'saved result export never invokes model again');
    for (const width of [640, 1200]) for (const theme of ['light', 'dark']) {
      await page.setViewportSize({ width, height: 900 });
      const buttonName = theme === 'light' ? '切换到亮色' : '切换到暗色';
      const toggle = rail().getByRole('button', { name: buttonName, exact: true });
      if (await toggle.count()) await toggle.click();
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${width}px ${theme} no viewport overflow`);
      await page.screenshot({ path: `${evidence}/sources-${width}-${theme}.png` });
    }
    await navigate('工作台'); await page.getByRole('button', { name: '运行离线样例' }).click();
    await page.getByText('离线作者样例自检完成', { exact: true }).waitFor();
    assert((await calls('source_extract')).length === 1 && (await calls('desktop_self_test')).length === 1, 'offline self test separate from model flow');

    await setup('missing'); await modelPage();
    const restore = page.getByRole('button', { name: '按历史快照恢复账本' });
    assert(await restore.isDisabled() && !(await page.getByRole('checkbox', { name: /我确认 2026-11-06/ }).isChecked()), 'history restore confirmation defaults false');
    await page.getByRole('button', { name: '打开加密凭据设置' }).click();
    assert((await calls('model_credential_store')).length === 1 && await restore.isDisabled(), 'credential dialog does not silently restore budget');
    await page.getByRole('checkbox', { name: /我确认 2026-11-06/ }).check(); await restore.click();
    assert((await calls('model_budget_restore')).length === 1 && (await calls('model_budget_restore'))[0].args.confirmed === true, 'explicit confirmed history restoration');
    assert(await page.getByText('33,185', { exact: true }).count() === 1 && await restore.count() === 0, 'restored cumulative history shown and restore entry removed');
    await setup('invalid-ledger'); await modelPage(); assert(await page.getByRole('button', { name: '按历史快照恢复账本' }).count() === 0, 'invalid ledger cannot be overwritten');

    await setup('failed-import'); await makeProject();
    await page.locator('#source-path').fill('D:\\bad.pdf'); await page.getByRole('button', { name: '导入并检查' }).click();
    await page.getByRole('alert').filter({ hasText: 'PDF_PARSE_FAILED' }).waitFor();
    assert((await calls('source_choices')).length === 0 && (await calls('source_extract')).length === 0 && await page.getByRole('button', { name: '导出待核验笔记' }).count() === 0, 'failed import blocks model and export');
    for (const scenario of ['needs-review', 'no-text', 'oversize']) {
      await setup(scenario); await makeProject(); await page.locator('#source-path').fill('D:\\scan.pdf'); await page.getByRole('button', { name: '导入并检查' }).click();
      await page.getByRole('heading', { name: '2. 选择要整理的原文' }).waitFor();
      if (scenario === 'needs-review') assert(await page.locator('.source-choice input').isDisabled(), 'unconfirmed scan region cannot be selected');
      if (scenario === 'no-text') assert(await page.getByText('没有可用于模型整理的文字区域。扫描页或无文字资料需先核对文字来源。').count() === 1, 'no-text source states unavailable text');
      if (scenario === 'oversize') { await page.locator('.source-choice input').check(); assert(await page.getByRole('button', { name: '查看将发送的完整原文' }).isDisabled(), '64KiB selection bound enforced'); }
      assert((await calls('source_extract')).length === 0, scenario + ' never invokes model');
    }
    for (const scenario of ['many', 'bulk-bytes']) {
      await setup(scenario); await makeProject(); await page.locator('#source-path').fill('D:/many.pdf'); await page.getByRole('button', { name: '导入并检查' }).click();
      await page.getByRole('heading', { name: '2. 选择要整理的原文' }).waitFor();
      assert(await page.locator('.source-choice input:checked').count() === 0, scenario + ' import never auto-selects');
      await page.getByRole('button', { name: '选择前64个可用片段' }).click();
      assert(await page.locator('.source-choice input:checked').count() === (scenario === 'many' ? 64 : 1), scenario + ' bulk respects 64 units/64KiB, selectable and nonempty bounds');
      assert((await calls('source_units')).length === 0 && (await calls('source_extract')).length === 0, scenario + ' bulk never previews or sends automatically');
      await page.getByRole('button', { name: '清空选择' }).click();
      assert(await page.locator('.source-choice input:checked').count() === 0 && await page.getByRole('button', { name: '查看将发送的完整原文' }).isDisabled(), scenario + ' clear removes selection and preflight');
    }
    await setup('text'); await modelPage(); await makeProject(); await importAndSelect();
    assert(await page.getByText('文本片段 1 · 原文范围：', { exact: false }).count() === 1, 'text Span locator is human-readable');
    await page.getByRole('button', { name: '生成候选（调用模型）', exact: true }).click(); await page.getByRole('heading', { name: '候选已生成 · 待核验' }).waitFor();
    assert((await calls('source_extract'))[0].args.selection[0].kind === 'text', 'real text selection DTO supported');
    await page.locator('#source-path').fill('D:/another source.txt');
    assert(await page.locator('.source-unit').count() === 0 && await page.getByRole('heading', { name: '候选已生成 · 待核验' }).count() === 0, 'changing source file clears confirmed text and result');
    await setup(); await makeProject();
    await page.getByText('恢复已保存的资料记录', { exact: true }).click(); await page.locator('#source-import-id').fill('import-1'); await page.getByRole('button', { name: '加载已保存资料' }).click();
    await page.getByRole('heading', { name: '2. 选择要整理的原文' }).waitFor();
    await page.locator('#result-root').fill('D:/saved API result'); await page.locator('#export-parent').fill('D:/new test output'); await page.getByRole('button', { name: '导出待核验笔记' }).click(); await page.getByRole('heading', { name: '待核验笔记已导出' }).waitFor();
    assert((await calls('source_import')).length === 0 && (await calls('source_extract')).length === 0, 'reloaded saved import and API result export uses zero model calls');
    for (const scenario of ['bad-choice-binding', 'bad-unit-binding', 'bad-extract-binding']) {
      await setup(scenario); await modelPage(); await makeProject();
      if (scenario === 'bad-choice-binding') { await page.locator('#source-path').fill('D:/source.pdf'); await page.getByRole('button', { name: '导入并检查' }).click(); }
      else if (scenario === 'bad-unit-binding') { await page.locator('#source-path').fill('D:/source.pdf'); await page.getByRole('button', { name: '导入并检查' }).click(); await page.getByRole('heading', { name: '2. 选择要整理的原文' }).waitFor(); await page.locator('.source-choice input').check(); await page.getByRole('button', { name: '查看将发送的完整原文' }).click(); }
      else { await importAndSelect(); await page.getByRole('button', { name: '生成候选（调用模型）', exact: true }).click(); }
      await page.getByRole('alert').filter({ hasText: 'DESKTOP_SOURCE_BINDING' }).waitFor();
      assert(await page.getByRole('heading', { name: '候选已生成 · 待核验' }).count() === 0 && (scenario === 'bad-extract-binding' || (await calls('source_extract')).length === 0), scenario + ' blocks mismatched success');
    }
    await setup('failed-extract'); await modelPage(); await makeProject(); await importAndSelect();
    await page.getByRole('button', { name: '生成候选（调用模型）', exact: true }).click(); await page.getByRole('heading', { name: '模型整理失败' }).waitFor();
    assert(await page.getByRole('button', { name: '导出待核验笔记' }).isDisabled(), 'failed extraction is not exportable');

    await setup('late'); await modelPage(); await makeProject(); await importAndSelect();
    await page.getByRole('button', { name: '生成候选（调用模型）', exact: true }).evaluate(button => { button.click(); button.click(); });
    await page.waitForFunction(() => typeof window.qa.release === 'function');
    assert((await calls('source_extract')).length === 1, 'same-tick double submit has one paid invocation');
    await navigate('工作台'); await navigate('资料');
    assert(await page.getByRole('button', { name: '模型正在整理…' }).isDisabled(), 'pending model request survives page changes');
    await makeProject('项目二 中文 spaces');
    assert(await page.locator('.source-choice').count() === 0 && await page.locator('#source-path').inputValue() === '', 'changing project clears imported source and result');
    await page.evaluate(() => window.qa.release());
    await page.waitForFunction(() => !document.querySelector('.workflow-operation'));
    assert(await page.getByRole('heading', { name: '候选已生成 · 待核验' }).count() === 0 && await page.locator('.workflow-project').innerText().then(text => text.includes('项目二')), 'late old-project response cannot overwrite current project');
    await setup('cancel'); await modelPage(); await makeProject(); await importAndSelect();
    await page.getByRole('button', { name: '生成候选（调用模型）', exact: true }).click(); await page.getByRole('button', { name: '取消当前操作' }).click();
    await page.getByRole('heading', { name: '模型整理失败' }).waitFor();
    assert((await calls('source_cancel')).length === 1 && (await calls('source_extract')).length === 1, 'cancel never repeats paid request');
    assert(consoleErrors.length === 0, 'no browser runtime or console errors');
    const result = { passed: true, kind: 'QA only; Tauri mock IPC, not native UI or real model/PDF verification', browser: 'Browser plugin not available; Playwright MCP', url: page.url(), title: await page.title(), checks, consoleErrors };
    return result;
  } catch (error) {
    await page.screenshot({ path: `${evidence}/qa-failure.png` });
    return { passed: false, error: String(error), checks, consoleErrors };
  }
}
