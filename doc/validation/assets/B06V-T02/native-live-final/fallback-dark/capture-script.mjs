// Actual installed Obsidian DOM, MathJax MathML/CHTML, image and link capture.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const [modulePath, inputPath] = process.argv.slice(2);
const { chromium } = await import(pathToFileURL(modulePath));
const input = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
const work = path.dirname(inputPath);
const browser = await chromium.connectOverCDP('http://localhost:' + input.port);
const obs = browser.contexts()[0].pages()[0];
const result = { status: 'running', scope: input.spec.scope, case: input.spec.case, notes: [], clicks: [] };
const norm = text => text.replace(/\\/g, '/').toLowerCase();
let owned = false;
try {
  await obs.waitForFunction(() => window.app?.workspace?.layoutReady && window.app.workspace.rootSplit, { timeout: 30000 });
  result.identity = await obs.evaluate(() => ({
    appVersion: require('@electron/remote').app.getVersion(),
    userData: require('@electron/remote').app.getPath('userData'),
    vaultPath: window.app.vault.adapter.basePath,
    communityPlugins: Object.keys(window.app.plugins.plugins),
    electron: process.versions.electron, chromium: process.versions.chrome
  }));
  assert.equal(norm(result.identity.userData), norm(input.profile));
  assert.equal(norm(result.identity.vaultPath), norm(input.vault));
  owned = true;
  assert.equal(result.identity.appVersion, '1.6.7');
  assert.deepEqual(result.identity.communityPlugins, []);
  assert(['light', 'dark'].includes(input.spec.theme), 'explicit bounded capture theme');
  await obs.waitForFunction(theme => document.body.classList.contains('theme-' + theme), input.spec.theme, { timeout: 15000 });
  result.identity.theme = await obs.evaluate(() => window.app.vault.getConfig('theme'));
  result.identity.bodyClass = await obs.evaluate(() => document.body.className);
  assert(result.identity.bodyClass.split(/\s+/).includes('theme-' + input.spec.theme), 'actual native body theme matches selected capture');
  result.openedOriginalFiles = await obs.evaluate(({ root, names }) => {
    const fs = require('fs');
    const path = require('path');
    const crypto = require('crypto');
    return Object.fromEntries(names.map(name => {
      if (path.basename(name) !== name) throw new Error('only explicitly named flat test files');
      const data = fs.readFileSync(path.join(root, name));
      return [name, { sha256: crypto.createHash('sha256').update(data).digest('hex'), byte_length: data.length }];
    }));
  }, { root: input.vault, names: Object.keys(input.source_pins) });
  assert.deepEqual(result.openedOriginalFiles, input.source_pins, 'native renderer reads exact copied original bytes');
  for (const name of input.spec.blocked_absent ?? []) {
    const absent = await obs.evaluate(async name => !(await window.app.vault.adapter.exists(name)), name);
    assert(absent, 'blocked output contains no affected success artifact: ' + name);
  }
  result.blockedAbsent = input.spec.blocked_absent ?? [];
  await obs.setViewportSize({ width: 1600, height: 1200 });
  await obs.mouse.move(20, 1150);
  const open = async file => {
    await obs.waitForFunction(p => window.app.vault.getAbstractFileByPath(p), file);
    await obs.evaluate(async p => window.app.workspace.getLeaf(false).openFile(window.app.vault.getAbstractFileByPath(p), { active: true, state: { mode: 'preview' } }), file);
    await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p && window.app.workspace.activeLeaf?.view?.getState()?.mode === 'preview', file);
    await obs.locator('.workspace-leaf.mod-active .markdown-preview-view').waitFor({ state: 'visible' });
    await obs.waitForTimeout(400);
  };
  const read = async () => obs.evaluate(() => {
    const view = document.querySelector('.workspace-leaf.mod-active .markdown-preview-view');
    const rect = e => { const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
    const glyphRect = e => {
      const boxes = Array.from(e.querySelectorAll('mjx-c')).map(rect).filter(r => r.width > 0 && r.height > 0);
      if (!boxes.length) throw new Error('visible mathematical component has no glyph area');
      const x = Math.min(...boxes.map(r => r.x)), y = Math.min(...boxes.map(r => r.y));
      return { x, y, width: Math.max(...boxes.map(r => r.x + r.width)) - x, height: Math.max(...boxes.map(r => r.y + r.height)) - y };
    };
    const position = e => ({ logicalTop: rect(e).y - rect(view).y + view.scrollTop, logicalLeft: rect(e).x - rect(view).x });
    const canonical = e => {
      const tag = e.localName;
      if (['mi', 'mn', 'mo', 'mtext'].includes(tag)) return [tag, e.textContent];
      const children = Array.from(e.children).map(canonical);
      if (['math', 'mrow', 'mstyle', 'semantics'].includes(tag)) return children.length === 1 ? children[0] : ['row', children];
      return [tag, children];
    };
    const chtmlTree = e => {
      const tag = e.localName.replace(/^mjx-/, '');
      if (['mi', 'mn', 'mo', 'mtext'].includes(tag)) {
        const glyph = Array.from(e.querySelectorAll('mjx-c')).map(c => {
          const match = /(?:^|\s)mjx-c([0-9A-F]+)(?:\s|$)/i.exec(c.className);
          if (!match) throw new Error('unrecognized visible MathJax glyph class');
          return String.fromCodePoint(parseInt(match[1], 16)).normalize('NFKD');
        }).join('');
        return [tag, glyph];
      }
      if (tag === 'mfrac') return ['mfrac', [chtmlTree(e.querySelector('mjx-num')), chtmlTree(e.querySelector('mjx-den'))]];
      if (tag === 'msubsup') {
        // Pinned CHTML lays out superscript first, spacer, then subscript.
        // Return the conventional semantic order: base, subscript, superscript.
        const scripts = Array.from(e.children[1].children).map(chtmlTree).filter(Boolean);
        if (scripts.length !== 2) throw new Error('unexpected CHTML combined-script layout');
        return ['msubsup', [chtmlTree(e.children[0]), scripts[1], scripts[0]]];
      }
      const children = Array.from(e.children).map(chtmlTree).filter(Boolean);
      if (['msub', 'msup', 'msubsup', 'msqrt'].includes(tag)) return [tag, children];
      if (['mspace', 'nstrut', 'dstrut', 'line', 'strut'].includes(tag) || !children.length) return null;
      return children.length === 1 ? children[0] : ['row', children];
    };
    return {
      activeFile: window.app.workspace.getActiveFile()?.path, dom: view.outerHTML,
      previewRect: rect(view), text: view.innerText,
      headings: Array.from(view.querySelectorAll('h1,h2')).map(e => ({ text: e.innerText, ...position(e) })),
      preformatted: Array.from(view.querySelectorAll('pre > code')).map(e => ({ text: e.textContent, innerText: e.innerText, whiteSpace: getComputedStyle(e).whiteSpace, ...position(e) })),
      paragraphs: Array.from(view.querySelectorAll('p')).map(e => ({ text: e.innerText, ...position(e) })),
      math: Array.from(view.querySelectorAll('mjx-container')).map(e => ({
        mathml: e.querySelector('mjx-assistive-mml math')?.outerHTML ?? null,
        tree: e.querySelector('mjx-assistive-mml math') ? canonical(e.querySelector('mjx-assistive-mml math')) : chtmlTree(e.querySelector('mjx-math')),
        treeSource: e.querySelector('mjx-assistive-mml math') ? 'actual MathML' : 'actual visible CHTML tags and glyph codepoints; layout wrappers collapsed',
        chtml: e.querySelector('mjx-math')?.outerHTML ?? null,
        rect: rect(e), ...position(e),
        glyphs: Array.from(e.querySelectorAll('mjx-c')).map(c => ({ classes: c.className, rect: rect(c) })),
        scriptGeometry: Array.from(e.querySelectorAll('mjx-msub,mjx-msup,mjx-msubsup')).map(s => {
          const base = glyphRect(s.children[0]);
          const script = s.children[1];
          const kinds = Array.from(script.children).filter(c => !['mjx-spacer','mjx-strut'].includes(c.localName));
          return { kind: s.localName, base, script: glyphRect(script), wrapperRect: rect(script),
            method: 'actual positive-area glyph union; inline wrapper rectangles describe line boxes rather than glyph placement',
            combined: s.localName === 'mjx-msubsup' ? { upper: glyphRect(kinds[0]), lower: glyphRect(kinds[1]) } : null };
        }),
        fractionGeometry: Array.from(e.querySelectorAll('mjx-mfrac')).map(f => ({ numerator: rect(f.querySelector('mjx-num')), denominator: rect(f.querySelector('mjx-den')) })),
        errors: Array.from(e.querySelectorAll('mjx-merror,merror')).map(e => e.textContent)
      })),
      images: Array.from(view.querySelectorAll('img')).map(e => ({
        src: e.getAttribute('src'), alt: e.alt, complete: e.complete,
        naturalWidth: e.naturalWidth, naturalHeight: e.naturalHeight,
        rect: rect(e), ...position(e)
      })),
      links: Array.from(view.querySelectorAll('a.internal-link')).map(e => ({
        text: e.innerText, target: e.getAttribute('data-href'),
        destination: window.app.metadataCache.getFirstLinkpathDest(e.getAttribute('data-href').split('#')[0], window.app.workspace.getActiveFile().path)?.path ?? null,
        ...position(e)
      })),
      activeTags: Array.from(view.querySelectorAll('script,iframe,object,embed')).map(e => e.outerHTML)
    };
  });
  const scrollTo = async top => {
    await obs.evaluate(top => { document.querySelector('.workspace-leaf.mod-active .markdown-preview-view').scrollTop = top; }, Math.max(0, top));
    await obs.waitForTimeout(180);
  };
  const whole = async () => {
    const sweep = async collect => {
      const samples = [];
      await scrollTo(0);
      for (let n = 0; n < 80; n++) {
        if (collect) samples.push(await read());
        const s = await obs.evaluate(() => { const v = document.querySelector('.workspace-leaf.mod-active .markdown-preview-view'); return { top: v.scrollTop, maximum: v.scrollHeight - v.clientHeight, height: v.clientHeight }; });
        if (s.top >= s.maximum - 1) return samples;
        await scrollTo(Math.min(s.maximum, s.top + s.height * .6));
      }
      assert.fail('bounded note did not complete scroll sweep');
    };
    await sweep(false);
    const samples = await sweep(true);
    const merge = (field, key) => {
      const values = [];
      for (const s of samples) for (const v of s[field]) if (!values.some(old => key(old) === key(v) && Math.abs(old.logicalTop - v.logicalTop) < 2 && Math.abs(old.logicalLeft - v.logicalLeft) < 2)) values.push(v);
      return values.sort((a, b) => a.logicalTop - b.logicalTop || a.logicalLeft - b.logicalLeft);
    };
    return { ...samples[0],
      headings: merge('headings', e => e.text), preformatted: merge('preformatted', e => e.text),
      paragraphs: merge('paragraphs', e => e.text), math: merge('math', e => JSON.stringify(e.tree)),
      images: merge('images', e => e.src), links: merge('links', e => e.target + '\u0000' + e.text),
      method: 'actual two native scroll sweeps; DOM blocks retained by source content and document position', samples };
  };
  for (const [index, note] of input.spec.notes.entries()) {
    await open(note.file);
    if (note.math_trees.length) await obs.waitForFunction(() => document.querySelector('.workspace-leaf.mod-active .markdown-preview-view mjx-container'), { timeout: 15000 });
    if (note.images.length) await obs.waitForFunction(() => Array.from(document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view img')).some(e => e.complete && e.naturalWidth > 0), { timeout: 15000 });
    const actual = await whole();
    const record = { file: note.file, expected: note, actual, status: 'checking', screenshot: 'note-' + index + '.png' };
    result.notes.push(record);
    actual.samples.forEach((s, n) => fs.writeFileSync(path.join(work, 'note-' + index + '-scroll-' + n + '.html'), s.dom, { flag: 'wx' }));
    actual.math.forEach((m, n) => {
      if (m.mathml) fs.writeFileSync(path.join(work, 'note-' + index + '-math-' + n + '.mathml'), m.mathml, { flag: 'wx' });
      if (m.chtml) fs.writeFileSync(path.join(work, 'note-' + index + '-math-' + n + '.chtml.html'), m.chtml, { flag: 'wx' });
    });
    await scrollTo(0);
    await obs.bringToFront();
    await obs.screenshot({ path: path.join(work, record.screenshot), fullPage: true, timeout: 60000 });
    assert.equal(actual.activeFile, note.file);
    assert.deepEqual(actual.math.map(m => m.tree), note.math_trees, 'actual MathJax tree retains signs, scripts, equation and fraction order');
    actual.math.forEach(m => {
      assert.equal(m.errors.length, 0, 'no MathJax error nodes');
      assert(m.chtml && m.rect.width > 0 && m.rect.height > 0, 'actual visible CHTML math');
      assert(m.glyphs.length > 0 && m.glyphs.every(g => g.classes === 'mjx-c2061' || (g.rect.width > 0 && g.rect.height > 0)), 'all visible glyphs have area; U2061 is the expected invisible function-application marker');
      m.scriptGeometry.forEach(s => {
        if (s.kind === 'mjx-msub') assert(s.script.y > s.base.y, 'actual subscript lies below its base');
        if (s.kind === 'mjx-msup') assert(s.script.y < s.base.y, 'actual superscript lies above its base');
        if (s.kind === 'mjx-msubsup') assert(s.combined.upper.y < s.base.y && s.combined.lower.y > s.base.y, 'actual combined scripts lie on their respective sides of the base');
      });
      m.fractionGeometry.forEach(f => assert(f.numerator.y < f.denominator.y, 'actual fraction numerator is above denominator'));
    });
    assert.equal(actual.images.length, note.images.length, 'actual embedded image count');
    actual.images.forEach((image, n) => {
      assert(image.complete && image.naturalWidth === note.images[n].width && image.naturalHeight === note.images[n].height, 'actual original PNG dimensions');
      assert(image.src.includes(encodeURI(note.images[n].file)) || image.src.includes(note.images[n].file), 'selected local original image URL');
      assert(!/^https?:/i.test(image.src), 'image is a local selected attachment');
      assert(image.rect.width > 0 && image.rect.height > 0);
    });
    const visible = actual.preformatted.map(e => e.innerText).concat(actual.paragraphs.map(e => e.text), actual.headings.map(e => e.text));
    for (const required of note.required_texts) assert(visible.some(text => text.includes(required)), 'native metadata text retained: ' + required);
    if (note.preformatted_exact) assert.deepEqual(actual.preformatted.map(e => e.text.replace(/\n$/, '')), note.preformatted_exact, 'native source metadata exact blocks');
    actual.preformatted.forEach(e => {
      assert.equal(e.text, e.innerText);
      assert(['pre', 'pre-wrap', 'break-spaces'].includes(e.whiteSpace));
    });
    assert.deepEqual(actual.activeTags, []);
    for (const target of note.link_destinations ?? []) assert(actual.links.some(l => l.destination === target), 'native local attachment resolves: ' + target);
    record.status = 'passed';
    for (const target of note.click_attachments ?? []) {
      await open(note.file);
      const observed = actual.links.find(link => link.destination === target);
      assert(observed, 'previous native scroll sweep observed the target attachment');
      await scrollTo(observed.logicalTop - 180);
      await obs.waitForFunction(target => Array.from(document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view a.internal-link')).some(e => e.getAttribute('data-href') === target), target);
      const link = obs.locator('.workspace-leaf.mod-active .markdown-preview-view a.internal-link').filter({ hasText: target });
      if (await link.count()) await link.first().click();
      else {
        const candidate = await obs.locator('.workspace-leaf.mod-active .markdown-preview-view a.internal-link').evaluateAll((links, target) => links.findIndex(l => l.getAttribute('data-href') === target), target);
        assert(candidate >= 0, 'actual attachment link exists');
        await obs.locator('.workspace-leaf.mod-active .markdown-preview-view a.internal-link').nth(candidate).click();
      }
      await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p, target);
      const clicked = await obs.evaluate(() => ({ path: window.app.workspace.getActiveFile().path, viewType: window.app.workspace.activeLeaf.view.getViewType() }));
      assert.equal(clicked.path, target);
      const expectedImage = input.spec.attachment_images[target];
      assert(expectedImage, 'clicked attachment has independently bound PNG dimensions');
      assert.equal(clicked.viewType, 'image', 'actual click opened the native image view');
      await obs.waitForFunction(({ target, expected }) => {
        const view = window.app.workspace.activeLeaf?.view;
        if (window.app.workspace.getActiveFile()?.path !== target || view?.getViewType() !== 'image') return false;
        const images = Array.from(view.containerEl.querySelectorAll('img'));
        return images.length === 1 && images[0].complete && images[0].naturalWidth === expected.width && images[0].naturalHeight === expected.height;
      }, { target, expected: expectedImage }, { timeout: 15000 });
      const readClickedImage = () => obs.evaluate(() => {
        const view = window.app.workspace.activeLeaf.view;
        const images = Array.from(view.containerEl.querySelectorAll('img'));
        if (images.length !== 1) throw new Error('active image view must contain exactly one PNG');
        const img = images[0], r = img.getBoundingClientRect(), style = getComputedStyle(img);
        return { activeFile: window.app.workspace.getActiveFile().path, viewType: view.getViewType(),
          src: img.currentSrc || img.src, complete: img.complete, naturalWidth: img.naturalWidth, naturalHeight: img.naturalHeight,
          rect: { x: r.x, y: r.y, width: r.width, height: r.height }, display: style.display, visibility: style.visibility };
      });
      const firstImage = await readClickedImage();
      await obs.waitForTimeout(200);
      const image = await readClickedImage();
      assert.equal(image.activeFile, target);
      assert.equal(image.viewType, 'image');
      assert(image.complete && image.naturalWidth === expectedImage.width && image.naturalHeight === expectedImage.height);
      assert.equal(firstImage.naturalWidth, image.naturalWidth);
      assert.equal(firstImage.naturalHeight, image.naturalHeight);
      assert(image.rect.width > 0 && image.rect.height > 0 && image.display !== 'none' && image.visibility !== 'hidden');
      assert(!/^https?:/i.test(image.src), 'clicked PNG remains a local selected attachment');
      assert(image.src.includes(target) || image.src.includes(encodeURI(target)), 'clicked native image URL names the selected PNG');
      clicked.image = { expected: expectedImage, actual: image, status: 'passed', method: 'active native image view after real click; fixed source dimensions' };
      const screenshot = 'attachment-click-' + result.clicks.length + '.png';
      await obs.bringToFront();
      await obs.screenshot({ path: path.join(work, screenshot), fullPage: true, timeout: 60000 });
      result.clicks.push({ source: note.file, target, actual: clicked, screenshot, actual_click: true });
    }
  }
  result.mathjax = await obs.evaluate(() => ({ version: window.MathJax?.version ?? null,
    resourceUrls: performance.getEntriesByType('resource').map(e => e.name).filter(name => /mathjax/i.test(name)) }));
  if (input.spec.require_mathjax_runtime) assert.equal(result.mathjax.version, input.spec.mathjax_version ?? '3.2.2');
  result.status = 'passed';
} catch (error) {
  result.status = 'failed'; result.error = { message: String(error), stack: error.stack }; process.exitCode = 1;
} finally {
  result.finishedUtc = new Date().toISOString();
  fs.writeFileSync(path.join(work, 'result.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  if (owned) {
    await obs.evaluate(() => setTimeout(() => require('@electron/remote').app.quit(), 100)).catch(() => {});
    await new Promise(resolve => setTimeout(resolve, 350));
  }
  await browser.close();
}
console.log(JSON.stringify({ status: result.status, case: result.case, identity: result.identity, mathjax: result.mathjax, notes: result.notes.map(n => ({ file: n.file, status: n.status })), clicks: result.clicks.length }));
