// Installed Obsidian capability probe; none of these notes are product output.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const [modulePath, workPath] = process.argv.slice(2);
const { chromium } = await import(pathToFileURL(modulePath));
const input = JSON.parse(fs.readFileSync(path.join(workPath, 'input.json'), 'utf8'));
const browser = await chromium.connectOverCDP('http://127.0.0.1:' + input.port);
const obs = browser.contexts()[0].pages()[0];
const result = { scope: 'installed Obsidian Mermaid capability only; self-authored non-scientific probes, not Noetome flow output or product acceptance', status: 'running', notes: [] };
let owned = false;
const normPath = s => s.replace(/\\/g, '/').toLowerCase();
const normText = s => s.replace(/\s+/gu, ' ').trim();
try {
  await obs.waitForFunction(() => window.app?.workspace?.layoutReady && window.app.workspace.rootSplit, { timeout: 30000 });
  result.identity = await obs.evaluate(() => ({
    appVersion: require('@electron/remote').app.getVersion(),
    userData: require('@electron/remote').app.getPath('userData'),
    vaultPath: window.app.vault.adapter.basePath,
    communityPlugins: Object.keys(window.app.plugins.plugins),
    electron: process.versions.electron, chromium: process.versions.chrome
  }));
  assert.equal(normPath(result.identity.userData), normPath(input.profile));
  assert.equal(normPath(result.identity.vaultPath), normPath(input.vault));
  owned = true;
  assert.deepEqual(result.identity.communityPlugins, []);
  await obs.setViewportSize({ width: 1600, height: 1200 });
  const open = async filePath => {
    await obs.waitForFunction(p => window.app.vault.getAbstractFileByPath(p), filePath);
    await obs.evaluate(async p => {
      await window.app.workspace.getLeaf(false).openFile(window.app.vault.getAbstractFileByPath(p), { active: true, state: { mode: 'preview' } });
    }, filePath);
    await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p && window.app.workspace.activeLeaf?.view?.getState()?.mode === 'preview', filePath);
    await obs.locator('.markdown-preview-view').waitFor({ state: 'visible' });
    await obs.waitForTimeout(400);
  };
  for (const [index, note] of input.notes.entries()) {
    await open(note.file);
    if (note.nodes.length) {
      await obs.waitForFunction(() => document.querySelector('.markdown-preview-view .mermaid svg,.markdown-preview-view .mermaid .error-icon,.markdown-preview-view .mermaid pre'), { timeout: 15000 }).catch(() => {});
      await obs.waitForTimeout(200);
    }
    const actual = await obs.evaluate(() => {
      const view = document.querySelector('.markdown-preview-view');
      const svg = view.querySelector('.mermaid svg');
      return {
        activeFile: window.app.workspace.getActiveFile()?.path,
        dom: view.outerHTML, text: view.innerText,
        listItems: Array.from(view.querySelectorAll('ol > li')).map(n => n.innerText),
        links: Array.from(view.querySelectorAll('a.internal-link')).map(a => ({ text: a.innerText, target: a.getAttribute('data-href') })),
        svg: svg?.outerHTML ?? null,
        nodes: svg ? Array.from(svg.querySelectorAll('g.node')).map(n => ({ id: n.id, text: n.querySelector('.label')?.textContent, visibleText: n.querySelector('.nodeLabel')?.innerText ?? null, whiteSpace: n.querySelector('.nodeLabel') ? getComputedStyle(n.querySelector('.nodeLabel')).whiteSpace : null, html: n.querySelector('.label')?.innerHTML, transform: n.getAttribute('transform'), rect: (() => { const r = n.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })() })) : [],
        edges: svg ? Array.from(svg.querySelectorAll('path.flowchart-link')).map(e => ({ id: e.id, classes: e.getAttribute('class'), markerEnd: e.getAttribute('marker-end'), d: e.getAttribute('d'), start: (() => { const p = e.getPointAtLength(0); return { x: p.x, y: p.y }; })(), end: (() => { const p = e.getPointAtLength(e.getTotalLength()); return { x: p.x, y: p.y }; })() })) : [],
        edgeLabels: svg ? Array.from(svg.querySelectorAll('g.edgeLabel')).filter(g => !g.parentElement?.closest('g.edgeLabel')).map(g => g.textContent).filter(text => text.trim()) : [],
        foreignObjectCount: svg?.querySelectorAll('foreignObject').length ?? 0,
        unexpectedActiveTags: svg ? Array.from(svg.querySelectorAll('script,iframe,img,image,a')).map(n => n.outerHTML) : [],
        mermaidErrorText: view.querySelector('.mermaid')?.innerText ?? null
      };
    });
    const checks = {
      svgPresent: note.nodes.length ? actual.svg !== null : actual.svg === null,
      nodesMatch: JSON.stringify(actual.nodes.map(n => normText(n.text))) === JSON.stringify(note.nodes.map(normText)),
      edgesMatch: actual.edges.length === note.edges,
      edgeLabelsMatch: JSON.stringify(actual.edgeLabels.map(normText)) === JSON.stringify(note.edgeLabels.map(normText)),
      arrowsPresent: actual.edges.every(e => Boolean(e.markerEnd)),
      listMatches: JSON.stringify(actual.listItems.map(normText)) === JSON.stringify(note.list.map(normText)),
      noActiveTags: actual.unexpectedActiveTags.length === 0
    };
    const visualWhitespacePreserved = actual.nodes.every((node, index) => node.visibleText === null || node.visibleText === note.nodes[index]);
    const record = { name: note.name, file: note.file, expected: note, actual, checks, visualWhitespacePreserved, screenshot: 'note-' + index + '.png' };
    result.notes.push(record);
    if (actual.svg) fs.writeFileSync(path.join(workPath, 'note-' + index + '.svg'), actual.svg);
    fs.writeFileSync(path.join(workPath, 'note-' + index + '.html'), actual.dom);
    await obs.screenshot({ path: path.join(workPath, record.screenshot), fullPage: true });
    if (note.required) assert(Object.values(checks).every(Boolean), 'required baseline failed: ' + note.name + ' ' + JSON.stringify(checks));
  }
  result.mermaidRuntime = await obs.evaluate(async () => {
    let exports = {};
    let instance = window.mermaid;
    let loaderError = null;
    try { exports = require('obsidian'); } catch (error) { loaderError = String(error); }
    if (!instance && typeof exports.loadMermaid === 'function') {
      try { instance = await exports.loadMermaid(); } catch (error) { loaderError = String(error); }
    }
    let config = null;
    try { config = instance?.mermaidAPI?.getConfig?.() ?? instance?.getConfig?.() ?? null; } catch (_) {}
    return {
      exposedVersion: instance?.version ?? instance?.default?.version ?? null,
      exportedKeys: instance ? Object.keys(instance) : [],
      loaderAvailable: typeof exports.loadMermaid === 'function', loaderError,
      config,
      resourceUrls: performance.getEntriesByType('resource').map(e => e.name).filter(name => /mermaid/i.test(name))
    };
  });
  const sourceUrl = result.mermaidRuntime.resourceUrls.find(url => /\.js/.test(url));
  if (sourceUrl) {
    const source = await obs.evaluate(async url => (await fetch(url)).text(), sourceUrl);
    fs.writeFileSync(path.join(workPath, 'mermaid-runtime-loaded.js'), source);
    result.mermaidRuntime.sourceUrl = sourceUrl;
    result.mermaidRuntime.sourcePrefix = source.slice(0, 1200);
  }
  result.navigation = [];
  for (const name of ['02-条件分支.md', '04-文字回退.md']) {
    await open(name);
    const link = await obs.locator('.markdown-preview-view a.internal-link').first().getAttribute('data-href');
    await obs.locator('.markdown-preview-view a.internal-link').first().click();
    const target = link.split('#')[0] + '.md';
    await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p, target);
    result.navigation.push({ source: name, target, link, actualClick: true });
  }
  result.status = 'completed';
} catch (error) {
  result.status = 'failed'; result.error = { message: String(error), stack: error.stack }; process.exitCode = 1;
} finally {
  result.finishedUtc = new Date().toISOString();
  fs.writeFileSync(path.join(workPath, 'result.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  if (owned) {
    await obs.evaluate(() => setTimeout(() => require('@electron/remote').app.quit(), 100));
    await new Promise(resolve => setTimeout(resolve, 300));
  }
  await browser.close();
}
console.log(JSON.stringify({ status: result.status, notes: result.notes.map(n => ({ name: n.name, checks: n.checks })), mermaidRuntime: result.mermaidRuntime?.exposedVersion, workPath }));
