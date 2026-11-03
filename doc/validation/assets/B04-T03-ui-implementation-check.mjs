import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.NOETOME_PLAYWRIGHT_PACKAGE);
const phase = process.argv[2];
const prefix = `doc/validation/assets/B04-T03-ui-implementation-${phase}`;
const results = [];
async function check(name, body) {
  try { await body(); results.push({ name, status: 'passed' }); }
  catch (error) { results.push({ name, status: 'failed', error: error.message }); }
}
const browser = await chromium.launch({ channel: 'msedge', headless: true });
const context = await browser.newContext({ viewport: { width: 640, height: 600 } });
context.setDefaultTimeout(2500);
const page = await context.newPage();
await page.goto('http://127.0.0.1:1420');
await page.locator('.app-menu-bar').waitFor();
await check('native configuration removes system title row', async () => {
  const configuration = JSON.parse(readFileSync('crates/noetome-desktop/tauri.conf.json'));
  assert.equal(configuration.app.windows.find(w => w.label === 'main').decorations, false);
});
await check('browser host absence disables all three same-row controls at 640px', async () => {
  const controls = page.locator('.app-window-controls button');
  assert.equal(await controls.count(), 3);
  for (const name of ['最小化', '最大化', '关闭窗口']) {
    assert.equal(await page.getByRole('button', { name, exact: true }).isDisabled(), true);
  }
  const bounds = await page.locator('.app-menu-bar').boundingBox();
  assert.equal(bounds.y, 0); assert.equal(bounds.height, 36);
  for (const control of await controls.all()) {
    const b = await control.boundingBox();
    assert.ok(b.y >= 0 && b.y + b.height <= 36 && b.x + b.width <= 640);
  }
});
await check('header remains visible when real page content scrolls', async () => {
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  assert.ok(await page.evaluate(() => window.scrollY > 0));
  assert.equal((await page.locator('.app-menu-bar').boundingBox()).y, 0);
  await page.evaluate(() => window.scrollTo(0, 0));
});
await check('blank drag surface excludes interactive descendants and Tauri automatic handler', async () => {
  const drag = page.locator('.app-window-drag-area');
  assert.equal(await drag.count(), 1);
  assert.ok((await drag.boundingBox()).width >= 24);
  assert.equal(await drag.locator('button, [role="menuitem"]').count(), 0);
  assert.equal(await page.locator('[data-tauri-drag-region]').count(), 0);
});
await page.screenshot({ path: `${prefix}-browser-640.png`, fullPage: true });
await check('existing menu keyboard and sidebar toggle still work without native host', async () => {
  const file = page.getByRole('menuitem', { name: 'File', exact: true });
  await file.focus(); await page.keyboard.press('ArrowDown');
  assert.equal(await page.getByRole('menu').count(), 1);
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('menu').count(), 0);
  assert.equal(await file.evaluate(el => el === document.activeElement), true);
  const sidebar = page.getByRole('button', { name: '切换侧栏' });
  await sidebar.click(); assert.equal(await sidebar.getAttribute('aria-expanded'), 'false');
  await sidebar.click(); assert.equal(await sidebar.getAttribute('aria-expanded'), 'true');
});
await context.close();
// Native IPC is the external boundary. This test double never proves OS operations.
const nativeContext = await browser.newContext({ viewport: { width: 1200, height: 800 } });
nativeContext.setDefaultTimeout(2500);
await nativeContext.addInitScript(() => {
  window.isTauri = true;
  let callbackId = 0;
  const callbacks = new Map();
  const listeners = new Map();
  window.uiTest = { calls: [], maximized: false, fail: null };
  window.uiTest.resize = value => {
    window.uiTest.maximized = value;
    for (const [id, item] of listeners) if (item.event === 'tauri://resize') {
      callbacks.get(item.handler)?.({ event: item.event, id, payload: { width: 1200, height: 800 } });
    }
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_, id) => listeners.delete(id) };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
    transformCallback: callback => { callbacks.set(++callbackId, callback); return callbackId; },
    invoke: async (command, args) => {
      window.uiTest.calls.push({ command, args });
      if (command === window.uiTest.fail) throw new Error('controlled window operation failure');
      if (command === 'plugin:event|listen') { const id = ++callbackId; listeners.set(id, args); return id; }
      if (command === 'plugin:event|unlisten') return;
      if (command === 'plugin:window|is_maximized') return window.uiTest.maximized;
      if (command === 'plugin:window|toggle_maximize') { window.uiTest.maximized = !window.uiTest.maximized; return; }
      if (['plugin:window|minimize', 'plugin:window|close', 'plugin:window|start_dragging'].includes(command)) return;
      if (command === 'get_close_behavior') return 'hide_to_tray';
      throw new Error(`unsupported test IPC ${command}`);
    }
  };
});
const native = await nativeContext.newPage();
await native.goto('http://127.0.0.1:1420');
await native.locator('.app-menu-bar').waitFor();
await check('maximize state and restore name follow native query and resize', async () => {
  const maximize = native.getByRole('button', { name: '最大化', exact: true });
  await maximize.waitFor();
  await maximize.click({ timeout: 2500 });
  await native.getByRole('button', { name: '还原', exact: true }).waitFor({ timeout: 2500 });
  await native.evaluate(() => window.uiTest.resize(false));
  await maximize.waitFor({ timeout: 2500 });
  assert.equal(await maximize.isDisabled(), false);
});
await check('keyboard min/close emit current-window commands and exclude dragging', async () => {
  await native.evaluate(() => { window.uiTest.calls = []; });
  for (const name of ['最小化', '关闭窗口']) {
    const button = native.getByRole('button', { name, exact: true });
    await button.focus(); await native.keyboard.press('Enter');
  }
  const calls = await native.evaluate(() => window.uiTest.calls);
  for (const command of ['plugin:window|minimize', 'plugin:window|close']) {
    assert.ok(calls.some(call => call.command === command && call.args.label === 'main'));
  }
  assert.ok(!calls.some(call => call.command === 'plugin:window|start_dragging'));
});
await check('blank double click toggles once; menu and navigation never drag', async () => {
  await native.evaluate(() => { window.uiTest.calls = []; });
  const drag = native.locator('.app-window-drag-area');
  await drag.dblclick({ timeout: 2500 });
  await native.getByRole('button', { name: '还原', exact: true }).waitFor({ timeout: 2500 });
  const calls = await native.evaluate(() => window.uiTest.calls);
  assert.equal(calls.filter(c => c.command === 'plugin:window|toggle_maximize').length, 1);
  assert.equal(calls.filter(c => c.command === 'plugin:window|start_dragging').length, 1);
  await native.evaluate(() => { window.uiTest.calls = []; });
  await native.getByRole('menuitem', { name: 'File', exact: true }).click();
  await native.keyboard.press('Escape');
  await native.getByRole('button', { name: '切换侧栏' }).click();
  assert.ok(!(await native.evaluate(() => window.uiTest.calls)).some(c => c.command === 'plugin:window|start_dragging'));
});
await check('native failure is visible below fixed 36px header and retry recovers', async () => {
  await native.evaluate(() => { window.uiTest.fail = 'plugin:window|minimize'; });
  await native.getByRole('button', { name: '最小化', exact: true }).click({ timeout: 2500 });
  const error = native.getByRole('alert');
  await error.waitFor({ timeout: 2500 });
  assert.match(await error.textContent(), /最小化.*controlled window operation failure/);
  assert.ok((await error.boundingBox()).y >= 36);
  assert.equal((await native.locator('.app-menu-bar').boundingBox()).height, 36);
  await native.screenshot({ path: `${prefix}-controlled-error.png`, fullPage: true });
  await native.evaluate(() => { window.uiTest.fail = null; });
  await native.getByRole('button', { name: '最小化', exact: true }).click();
  assert.equal(await native.getByRole('alert').count(), 0);
});
await check('failed post-toggle query disables uncertain state until resize query recovers', async () => {
  await native.evaluate(() => { window.uiTest.fail = 'plugin:window|is_maximized'; });
  const toggle = native.locator('.app-window-controls button').nth(1);
  await toggle.click();
  await native.getByRole('alert').waitFor();
  assert.equal(await toggle.isDisabled(), true);
  await native.evaluate(() => { window.uiTest.fail = null; window.uiTest.resize(false); });
  await native.getByRole('button', { name: '最大化', exact: true }).waitFor();
  assert.equal(await toggle.isDisabled(), false);
});
await nativeContext.close(); await browser.close();
const files = ['apps/desktop/src/AppMenuBar.tsx', 'apps/desktop/src/styles.css', 'crates/noetome-desktop/tauri.conf.json', 'crates/noetome-desktop/capabilities/main.json'];
try { readFileSync('apps/desktop/src/WindowControls.tsx'); files.push('apps/desktop/src/WindowControls.tsx'); } catch {}
const evidence = { phase, at: new Date().toISOString(), native_os_proof: false, results, sources: Object.fromEntries(files.map(file => [file, createHash('sha256').update(readFileSync(file)).digest('hex')])) };
writeFileSync(`${prefix}-checks.json`, JSON.stringify(evidence, null, 2));
console.log(JSON.stringify(evidence, null, 2));
process.exitCode = results.some(result => result.status === 'failed') ? 1 : 0;
