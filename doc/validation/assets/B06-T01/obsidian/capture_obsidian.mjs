// Real installed Obsidian only. This harness never renders its own Markdown.
// Usage: node capture_obsidian.mjs <playwright-core index.mjs> <visual-input.json>
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';

const [playwrightModule, inputPath] = process.argv.slice(2);
const { chromium } = await import(pathToFileURL(playwrightModule));
const input = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
const work = path.dirname(inputPath);
const resultPath = path.join(work, 'render-result.json');
assert(!fs.existsSync(resultPath), 'do not overwrite validation history');
const result = { status: 'running', caseName: input.case_name, rendered: [], clicks: [] };
const browser = await chromium.connectOverCDP('http://127.0.0.1:' + input.port);
const obs = browser.contexts()[0].pages()[0];
let owned = false;
const normalizePath = s => s.replace(/\\+/g, '/').toLowerCase();
const normalizeText = s => s.replace(/\r/g, '').replace(/\s+/gu, ' ').trim();
const payload = x => normalizeText(x.text + (x.links.length ? ' ' + x.links.map(l => l.display_text).join(' ') : ''));

try {
  await obs.waitForFunction(() => window.app?.vault?.adapter?.basePath, { timeout: 30000 });
  result.identity = await obs.evaluate(() => ({
    userData: require('@electron/remote').app.getPath('userData'),
    appVersion: require('@electron/remote').app.getVersion(),
    vaultPath: window.app.vault.adapter.basePath,
    communityPlugins: Object.keys(window.app.plugins.plugins),
    electron: window.process.versions.electron, chrome: window.process.versions.chrome,
    title: document.title
  }));
  assert.equal(normalizePath(result.identity.userData), normalizePath(input.profile));
  assert.equal(normalizePath(result.identity.vaultPath), normalizePath(input.vault));
  owned = true;
  assert.deepEqual(result.identity.communityPlugins, []);
  await obs.waitForFunction(() => window.app.workspace.layoutReady && window.app.workspace.rootSplit, { timeout: 30000 });
  await obs.setViewportSize({ width: 1600, height: 1200 });
  const open = async filePath => {
    await obs.waitForFunction(filePath => window.app.vault.getAbstractFileByPath(filePath), filePath, { timeout: 30000 });
    await obs.evaluate(async filePath => {
      const file = window.app.vault.getAbstractFileByPath(filePath);
      if (!file) throw Error('Selected own Vault note missing: ' + filePath);
      await window.app.workspace.getLeaf(false).openFile(file, { active: true, state: { mode: 'preview' } });
    }, filePath);
    await obs.waitForFunction(filePath => window.app.workspace.getActiveFile()?.path === filePath &&
      window.app.workspace.activeLeaf?.view?.getState()?.mode === 'preview', filePath);
    await obs.locator('.markdown-preview-view').waitFor({ state: 'visible' });
    await obs.waitForTimeout(200);
  };
  const read = async () => obs.evaluate(() => {
    const view = document.querySelector('.markdown-preview-view');
    return {
      activeFile: window.app.workspace.getActiveFile()?.path,
      activeView: window.app.workspace.activeLeaf?.view?.getState(), dom: view.outerHTML,
      headings: Array.from(view.querySelectorAll('h1,h2')).map(h => ({ tag: h.tagName, text: h.innerText, textContent: h.textContent, brCount: h.querySelectorAll('br').length, dataHeading: h.getAttribute('data-heading') })),
      tables: Array.from(view.querySelectorAll('table')).map(t => Array.from(t.querySelectorAll('tr')).map(r => Array.from(r.children).map(c => c.innerText))),
      paragraphs: Array.from(view.querySelectorAll('p')).map(p => p.innerText),
      links: Array.from(view.querySelectorAll('a.internal-link')).map(a => ({ text: a.innerText, target: a.getAttribute('data-href'), className: a.className })),
      brCount: view.querySelectorAll('br').length, bodyText: view.innerText
    };
  });
  for (let i = 0; i < input.plan.notes.length; i++) {
    const note = input.plan.notes[i];
    await open(note.output_path);
    const actual = await read();
    const screenshot = 'note-' + i + '.png';
    await obs.screenshot({ path: path.join(work, screenshot), fullPage: true });
    const record = { note: note.output_path, actual, screenshot, status: 'checking' };
    result.rendered.push(record);
    assert.equal(actual.activeFile, note.output_path, 'actual active native note');
    assert.deepEqual(actual.headings.map(h => normalizeText(h.text)), [normalizeText(note.title), normalizeText(note.heading)], 'native headings');
    if (input.expected.literal_label && note.title === input.expected.literal_label) {
      assert.equal(actual.headings[0].brCount, 1, 'multiline label has one real rendered break');
      assert(actual.headings[0].textContent.includes('\t'), 'literal tab remains a DOM text character');
      assert.equal(actual.headings[0].textContent, input.expected.literal_label.replace('\n', ''), 'label text is not parsed as markup');
      record.literalLabelCheck = 'passed';
    }
    const tables = note.nodes.filter(n => n.kind === 'table');
    const expectedTables = tables.map(t => [t.columns.map(c => normalizeText(c.label)), ...t.rows.map(r => r.cells.map(payload))]);
    assert.deepEqual(actual.tables.map(t => t.map(r => r.map(normalizeText))), expectedTables, 'native table cells');
    assert.deepEqual(actual.paragraphs.map(normalizeText), note.nodes.filter(n => n.kind === 'paragraph').map(payload), 'native paragraphs');
    const expectedLinks = note.nodes.flatMap(n => n.kind === 'table' ? n.rows.flatMap(r => r.cells.flatMap(c => c.links)) : n.links);
    assert.deepEqual(actual.links.map(l => ({ text: normalizeText(l.text), target: l.target })), expectedLinks.map(l => ({ text: normalizeText(l.display_text), target: l.target_path.slice(0, -3) + '#' + l.target_heading })), 'native links');
    record.status = 'passed';
    if (i === 0) for (let j = 0; j < expectedLinks.length; j++) {
      const link = expectedLinks[j];
      await open(note.output_path);
      await obs.locator('.markdown-preview-view a.internal-link').nth(j).click();
      await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p, link.target_path);
      await obs.locator('.markdown-preview-view').waitFor({ state: 'visible' });
      await obs.waitForTimeout(150);
      const after = await read();
      assert.equal(after.activeFile, link.target_path);
      assert(after.headings.some(h => normalizeText(h.text) === normalizeText(link.target_heading)));
      const screenshot = 'click-' + j + '.png';
      await obs.screenshot({ path: path.join(work, screenshot), fullPage: true });
      result.clicks.push({ source: note.output_path, linkIndex: j, expectedTarget: link.target_path, actualTarget: after.activeFile, heading: link.target_heading, screenshot, targetDom: after.dom, actualLocatorClick: true });
    }
  }
  result.status = 'passed';
} catch (error) {
  result.status = 'failed';
  result.error = { message: String(error), stack: error.stack };
  process.exitCode = 1;
} finally {
  result.scope = 'Actual installed Obsidian, isolated profile, actual DOM and link clicks; no complete version matrix or scientific quality claim';
  result.finishedUtc = new Date().toISOString();
  fs.writeFileSync(resultPath, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  if (owned) {
    // Quits only the instance whose exact userData and Vault identity we checked.
    await obs.evaluate(() => setTimeout(() => require('@electron/remote').app.quit(), 100));
    await new Promise(resolve => setTimeout(resolve, 300));
  }
  await browser.close();
}
console.log(JSON.stringify({ status: result.status, caseName: result.caseName, notes: result.rendered.length, clicks: result.clicks.length, resultPath }));
