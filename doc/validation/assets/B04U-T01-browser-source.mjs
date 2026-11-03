import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import crypto from 'node:crypto';

export async function verifyHeader(page, { mode = 'browser', output }) {
  const report = { mode, started_at: new Date().toISOString(), url: page.url(), checks: [], screenshots: [], errors: [] };
  const check = async (name, fn) => {
    try { await fn(); report.checks.push({ name, passed: true }); }
    catch (error) { report.checks.push({ name, passed: false, error: String(error) }); }
  };
  const menu = name => page.getByRole('menuitem', { name, exact: true });
  const open = async name => { await menu(name).click(); await page.getByRole('menu', { name, exact: true }).waitFor(); };
  const action = name => page.getByRole('menu').getByRole('menuitem', { name, exact: true });
  const heading = () => page.locator('main h1');
  const focused = () => page.evaluate(() => document.activeElement?.textContent?.trim());
  const expectHeading = async text => { await heading().filter({ hasText: text }).waitFor(); };
  page.on('pageerror', error => report.errors.push(String(error)));
  await page.reload();
  await page.locator('.app-menu-bar').waitFor();
  await check('toolbar: removal, three controls, four menus, initial history bounds', async () => {
    assert.equal(await page.locator('.breadcrumb, .topbar, .build-tag').count(), 0);
    assert.equal(JSON.stringify(await page.getByRole('menubar').locator(':scope > button').allTextContents()), JSON.stringify(['File', 'Edit', 'View', 'Help']));
    assert(await page.getByRole('button', { name: '后退', exact: true }).isDisabled());
    assert(await page.getByRole('button', { name: '前进', exact: true }).isDisabled());
    assert.equal(await page.getByRole('button', { name: '切换侧栏', exact: true }).getAttribute('aria-expanded'), 'true');
  });
  await check('history: sidebar and main actions, back/forward, no duplicate and branch truncation', async () => {
    await page.locator('#app-sidebar').getByRole('button', { name: '运行检查', exact: true }).click();
    await expectHeading('检查本地连接');
    await page.locator('#app-sidebar').getByRole('button', { name: '运行检查', exact: true }).click();
    await page.getByRole('button', { name: '后退', exact: true }).click();
    await expectHeading('为理解');
    assert(await page.getByRole('button', { name: '后退', exact: true }).isDisabled());
    await page.getByRole('button', { name: '前进', exact: true }).click();
    await expectHeading('检查本地连接');
    await page.getByRole('button', { name: '后退', exact: true }).click();
    await page.locator('#app-sidebar').getByRole('button', { name: '关于', exact: true }).click();
    await expectHeading('让知识');
    assert(await page.getByRole('button', { name: '前进', exact: true }).isDisabled());
    await page.getByRole('button', { name: '后退', exact: true }).click();
    await page.getByRole('button', { name: '检查本地运行环境', exact: true }).click();
    await expectHeading('检查本地连接');
  });
  await check('sidebar: hidden width released, actual aria state, View restore', async () => {
    const before = await page.locator('.main-layout').boundingBox();
    await page.getByRole('button', { name: '切换侧栏', exact: true }).click();
    assert(!await page.locator('#app-sidebar').isVisible());
    assert.equal(await page.getByRole('button', { name: '切换侧栏', exact: true }).getAttribute('aria-expanded'), 'false');
    const after = await page.locator('.main-layout').boundingBox();
    assert(after.width > before.width);
    await open('View');
    const toggle = page.getByRole('menuitemcheckbox', { name: '显示侧栏', exact: true });
    assert.equal(await toggle.getAttribute('aria-checked'), 'false');
    await toggle.click();
    assert(await page.locator('#app-sidebar').isVisible());
  });
  await check('keyboard: roving menubar, first/last, arrows, Escape focus restore', async () => {
    await menu('File').focus();
    await page.keyboard.press('ArrowRight'); assert.equal(await focused(), 'Edit');
    await page.keyboard.press('End'); assert.equal(await focused(), 'Help');
    await page.keyboard.press('Home'); assert.equal(await focused(), 'File');
    await page.keyboard.press('ArrowDown'); assert.equal(await focused(), '工作台');
    await page.keyboard.press('End'); assert.equal(await focused(), '窗口设置');
    await page.keyboard.press('ArrowUp'); assert.equal(await focused(), '运行检查');
    await page.keyboard.press('ArrowRight'); assert.equal(await focused(), '全选');
    await page.keyboard.press('Escape'); assert.equal(await focused(), 'Edit');
    assert.equal(await page.getByRole('menu').count(), 0);
    await menu('File').focus(); await page.keyboard.press('ArrowUp'); assert.equal(await focused(), '窗口设置');
    await page.keyboard.press('Escape');
  });
  await check('keyboard: Enter/Space activation, disabled item skip, Tab exit', async () => {
    await menu('Edit').focus(); await page.keyboard.press('Enter');
    assert.equal(await focused(), '全选');
    assert(await action('复制').isDisabled());
    await page.keyboard.press('ArrowDown'); assert.equal(await focused(), '全选');
    await page.keyboard.press('Escape');
    await menu('Help').focus(); await page.keyboard.press('Space');
    assert.equal(await focused(), '查看帮助');
    await page.keyboard.press('Tab');
    assert.equal(await page.getByRole('menu').count(), 0);
    assert.notEqual(await focused(), '查看帮助');
  });
  await check('dismissal: pointer outside and focus outside', async () => {
    await open('File'); await heading().click(); assert.equal(await page.getByRole('menu').count(), 0);
    await open('Help'); await page.locator('main').focus(); assert.equal(await page.getByRole('menu').count(), 0);
  });
  await check('File: workspace and window settings focus, actual settings state', async () => {
    await open('File'); await action('工作台').click(); await expectHeading('为理解');
    assert.equal(await page.evaluate(() => document.activeElement?.id), 'main-content');
    await open('File'); await action('窗口设置').click(); await expectHeading('让知识');
    assert.equal(await page.evaluate(() => document.activeElement?.id), 'window-settings-title');
    if (mode === 'native') {
      await page.locator('#close-behavior:not(:disabled)').waitFor();
      assert(['hide_to_tray', 'exit'].includes(await page.locator('#close-behavior').inputValue()));
      assert.match(await page.locator('#window-settings-status').innerText(), /当前设置/);
      assert(!await page.getByRole('button', { name: '保存设置', exact: true }).isEnabled());
    } else {
      await page.getByRole('alert').filter({ hasText: 'DESKTOP_SETTINGS_UNAVAILABLE' }).waitFor();
    }
  });
  await check('Help: help and version invoke existing runtime with honest result', async () => {
    for (const name of ['查看帮助', '查看版本']) {
      await open('Help'); await action(name).click(); await expectHeading('检查本地连接');
      await page.locator('.console-status').filter({ hasText: mode === 'native' ? '执行成功' : '执行失败' }).waitFor();
      const text = await page.getByLabel('CLI 运行结果', { exact: true }).innerText();
      if (mode === 'native') {
        assert.match(text, /noetome/i);
        if (name === '查看版本') assert.match(text, /0\.1\.0/);
      } else assert.match(text, /DESKTOP_IPC_ERROR/);
    }
    await open('Help'); await action('关于 Noetome').click(); await expectHeading('让知识');
  });
  if (mode === 'browser') {
    await check('Edit: select main content; controlled clipboard success and failure', async () => {
      await open('File'); await action('工作台').click();
      await page.evaluate(() => {
        window.__headerClipboardWrites = [];
        Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async text => window.__headerClipboardWrites.push(text) } });
      });
      await open('Edit'); await action('全选').click();
      const selected = await page.evaluate(() => window.getSelection().toString()); assert.match(selected, /为理解/);
      await open('Edit'); assert(await action('复制').isEnabled()); await action('复制').click();
      assert.equal(JSON.stringify(await page.evaluate(() => window.__headerClipboardWrites)), JSON.stringify([selected]));
      await page.evaluate(() => { navigator.clipboard.writeText = async () => { throw new Error('controlled-clipboard-denied'); }; });
      await open('Edit'); await action('全选').click(); await open('Edit'); await action('复制').click();
      await page.getByRole('alert').filter({ hasText: '复制' }).waitFor();
      assert.equal((await page.evaluate(() => window.__headerClipboardWrites)).length, 1);
    });
    for (const width of [360, 640, 1200]) {
      await check(`layout: ${width}px toolbar and all dropdowns within viewport`, async () => {
        await page.setViewportSize({ width, height: 800 });
        await open('File'); await action('工作台').click();
        const bounds = await page.locator('.app-menu-bar').boundingBox(); assert.equal(Math.round(bounds.width), width);
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
        for (const name of ['File', 'Edit', 'View', 'Help']) {
          await open(name); const box = await page.getByRole('menu', { name, exact: true }).boundingBox();
          assert(box.x >= 0 && box.x + box.width <= width + 1); await page.keyboard.press('Escape');
        }
        const path = `${output}/B04U-T01-browser-${width}.png`; await page.screenshot({ path, fullPage: false }); report.screenshots.push(path);
      });
    }
  }
  await check('no uncaught application JavaScript errors', async () => assert.deepEqual(report.errors, []));
  report.finished_at = new Date().toISOString();
  report.passed = report.checks.every(check => check.passed);
  const source = await fs.readFile(new URL(import.meta.url));
  report.verifier_sha256 = crypto.createHash('sha256').update(source).digest('hex');
  await fs.writeFile(`${output}/B04U-T01-${mode}-behavior.json`, JSON.stringify(report, null, 2) + '\n');
  return report;
}
