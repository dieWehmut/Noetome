// Read actual installed Obsidian output. Never render substitute Markdown here.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const [modulePath, inputPath] = process.argv.slice(2);
const { chromium } = await import(pathToFileURL(modulePath));
const input = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
const work = path.dirname(inputPath);
const result = { status: 'running', caseName: input.case_name, rendered: [], clicks: [] };
const browser = await chromium.connectOverCDP('http://127.0.0.1:' + input.port);
const obs = browser.contexts()[0].pages()[0];
const normPath = text => text.replace(/\\+/g, '/').toLowerCase();
const linksOf = block => block.kind === 'links' ? block.links : block.kind === 'ordered_list' ? block.items.flatMap(item => item.links) : [];
const expectedLink = link => ({ text: link.display_text, target: link.target_path.slice(0, -3) + '#' + link.target_heading });
const byId = values => [...values].sort((a, b) => a.id.localeCompare(b.id));
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
  assert.equal(normPath(result.identity.userData), normPath(input.profile));
  assert.equal(normPath(result.identity.vaultPath), normPath(input.vault));
  owned = true;
  assert.equal(result.identity.appVersion, '1.6.7');
  assert.deepEqual(result.identity.communityPlugins, []);
  assert.deepEqual(input.plan.document.process, input.expected.process);
  assert.deepEqual(input.plan.document.steps, byId(input.expected.nodes));
  assert.deepEqual(input.plan.document.relations, byId(input.expected.edges));
  await obs.setViewportSize({ width: 1600, height: 1200 });
  await obs.mouse.move(30, 1100);
  const captureScreenshot = async filename => {
    await obs.bringToFront();
    await obs.screenshot({ path: path.join(work, filename), fullPage: true, timeout: 60000 });
  };
  const open = async file => {
    await obs.waitForFunction(p => window.app.vault.getAbstractFileByPath(p), file);
    await obs.evaluate(async p => window.app.workspace.getLeaf(false).openFile(window.app.vault.getAbstractFileByPath(p), { active: true, state: { mode: 'preview' } }), file);
    await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p && window.app.workspace.activeLeaf?.view?.getState()?.mode === 'preview', file);
    await obs.locator('.workspace-leaf.mod-active .markdown-preview-view').waitFor({ state: 'visible' });
    await obs.waitForTimeout(350);
  };
  const read = async () => obs.evaluate(() => {
    const view = document.querySelector('.workspace-leaf.mod-active .markdown-preview-view');
    const rect = el => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
    const position = el => { const r = rect(el); return { logicalTop: r.y - rect(view).y + view.scrollTop, logicalLeft: r.x - rect(view).x }; };
    const link = a => ({ text: a.innerText, target: a.getAttribute('data-href'), ...position(a) });
    const ranges = el => {
      const result = []; const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      let node; let absolute = 0;
      while ((node = walker.nextNode())) {
        let offset = 0;
        for (const character of node.textContent) {
          const range = document.createRange(); range.setStart(node, offset); range.setEnd(node, offset + character.length);
          result.push({ offset: absolute + offset, character, codepoint: character.codePointAt(0), ...rect(range) });
          offset += character.length;
        }
        absolute += node.textContent.length;
      }
      return result;
    };
    const code = el => ({ text: el.textContent, innerText: el.innerText, whiteSpace: getComputedStyle(el).whiteSpace, tabSize: getComputedStyle(el).tabSize,
      glyphRanges: el.textContent.includes('\t') ? ranges(el) : [], rect: rect(el), ...position(el) });
    const svg = view.querySelector('.mermaid svg');
    return {
      activeFile: window.app.workspace.getActiveFile()?.path, dom: view.outerHTML, bodyText: view.innerText, previewRect: rect(view),
      headings: Array.from(view.querySelectorAll('h1,h2')).map(h => ({ tag: h.tagName, text: h.innerText, dataHeading: h.getAttribute('data-heading'), rect: rect(h), ...position(h) })),
      codeBlocks: Array.from(view.querySelectorAll('pre > code')).map(code),
      lists: Array.from(view.querySelectorAll('ol')).map(ol => ({ start: ol.start, type: getComputedStyle(ol).listStyleType, ...position(ol),
        items: Array.from(ol.children).map(li => ({ tag: li.tagName, value: li.getAttribute('value'), display: getComputedStyle(li).display,
          codes: Array.from(li.querySelectorAll('pre > code')).map(code), links: Array.from(li.querySelectorAll('a.internal-link')).map(link), rect: rect(li) })) })),
      links: Array.from(view.querySelectorAll('a.internal-link')).map(link),
      svg: svg?.outerHTML ?? null,
      nodes: svg ? Array.from(svg.querySelectorAll('g.node')).map(n => ({ id: n.id, text: n.querySelector('.label')?.textContent,
        innerText: n.querySelector('.nodeLabel')?.innerText, shape: n.querySelector(':scope > polygon') ? 'decision' : n.querySelector(':scope > rect') ? 'step' : 'unknown',
        rect: rect(n), glyphRanges: ranges(n.querySelector('.nodeLabel')) })) : [],
      edges: svg ? Array.from(svg.querySelectorAll('path.flowchart-link')).map(e => ({ id: e.id, classes: e.getAttribute('class'), markerEnd: e.getAttribute('marker-end'),
        start: (() => { const p = e.getPointAtLength(0); return { x: p.x, y: p.y }; })(),
        end: (() => { const p = e.getPointAtLength(e.getTotalLength()); return { x: p.x, y: p.y }; })(), d: e.getAttribute('d') })) : [],
      edgeLabels: svg ? Array.from(svg.querySelectorAll('g.edgeLabel')).filter(g => !g.parentElement?.closest('g.edgeLabel')).map(g => ({ text: g.textContent, innerText: g.querySelector('span')?.innerText })) : [],
      unexpectedActiveTags: svg ? Array.from(svg.querySelectorAll('script,iframe,img,image,a')).map(n => n.outerHTML) : [],
      mermaidError: view.querySelector('.mermaid .error-icon,.mermaid .error-text')?.outerHTML ?? null
    };
  });
  const scrollTo = async position => {
    await obs.evaluate(top => { document.querySelector('.workspace-leaf.mod-active .markdown-preview-view').scrollTop = top; }, Math.max(0, position));
    await obs.waitForTimeout(175);
  };
  const readWhole = async () => {
    const sweep = async collect => {
      const samples = [];
      await scrollTo(0);
      for (let iteration = 0; iteration < 80; iteration++) {
        if (collect) samples.push(await read());
        const scroll = await obs.evaluate(() => { const v = document.querySelector('.workspace-leaf.mod-active .markdown-preview-view'); return { top: v.scrollTop, height: v.clientHeight, maximum: v.scrollHeight - v.clientHeight }; });
        if (scroll.top >= scroll.maximum - 1) return samples;
        await scrollTo(Math.min(scroll.maximum, scroll.top + scroll.height * .6));
      }
      assert.fail('bounded note did not finish native scroll sweep');
    };
    // First visit lets Obsidian measure its virtualized blocks. The second
    // retains only actual DOM observations, merged by text and document position.
    await sweep(false);
    const samples = await sweep(true);
    const merge = (field, identity) => {
      const values = [];
      for (const sample of samples) for (const value of sample[field]) {
        if (!values.some(old => identity(old) === identity(value) && Math.abs(old.logicalTop - value.logicalTop) < 2 && Math.abs(old.logicalLeft - value.logicalLeft) < 2)) values.push(value);
      }
      return values.sort((a, b) => a.logicalTop - b.logicalTop || a.logicalLeft - b.logicalLeft);
    };
    const diagram = samples.find(sample => sample.svg) ?? samples[0];
    return { ...samples[0],
      headings: merge('headings', h => h.tag + h.text),
      codeBlocks: merge('codeBlocks', c => c.text),
      lists: merge('lists', l => JSON.stringify(l.items.map(i => i.codes.map(c => c.text)))),
      links: merge('links', l => l.target + '\u0000' + l.text),
      svg: diagram.svg, nodes: diagram.nodes, edges: diagram.edges, edgeLabels: diagram.edgeLabels,
      unexpectedActiveTags: diagram.unexpectedActiveTags, mermaidError: diagram.mermaidError,
      readbackMethod: 'two native scroll sweeps; actual DOM blocks merged by content and absolute document position',
      domSamples: samples.map(sample => ({ dom: sample.dom, previewRect: sample.previewRect, headings: sample.headings }))
    };
  };
  for (const [index, note] of input.plan.notes.entries()) {
    await open(note.output_path);
    const blocks = note.sections.flatMap(section => section.blocks);
    const hasDiagram = blocks.some(block => block.kind === 'diagram');
    if (hasDiagram) await obs.waitForFunction(() => document.querySelector('.workspace-leaf.mod-active .markdown-preview-view .mermaid svg'), { timeout: 15000 });
    const actual = await readWhole();
    await scrollTo(0);
    const screenshot = 'note-' + index + '.png';
    fs.writeFileSync(path.join(work, 'note-' + index + '.html'), actual.dom, { flag: 'wx' });
    actual.domSamples.forEach((sample, i) => fs.writeFileSync(path.join(work, 'note-' + index + '-scroll-' + i + '.html'), sample.dom, { flag: 'wx' }));
    if (actual.svg) fs.writeFileSync(path.join(work, 'note-' + index + '.svg'), actual.svg, { flag: 'wx' });
    const record = { note: note.output_path, actual, screenshot, status: 'checking', sectionScreenshots: [] };
    result.rendered.push(record);
    await captureScreenshot(screenshot);
    assert.equal(actual.activeFile, note.output_path);
    assert.deepEqual(actual.headings.map(h => h.text), [note.title, ...note.sections.map(s => s.heading)], 'native fixed headings');
    const expectedTexts = blocks.flatMap(block => block.kind === 'text' ? [block.content.text] : block.kind === 'ordered_list' ? block.items.map(item => item.text) : []);
    assert.equal(actual.codeBlocks.length, expectedTexts.length, 'all exact source-preserving blocks');
    actual.codeBlocks.forEach((code, i) => {
      assert(code.text === expectedTexts[i] || code.text === expectedTexts[i] + '\n', 'code block literal bytes ' + i);
      assert.equal(code.innerText, code.text, 'preformatted visible text retains source whitespace');
      assert(['pre', 'pre-wrap', 'break-spaces'].includes(code.whiteSpace), 'whitespace-preserving code style');
    });
    const expectedLists = blocks.filter(block => block.kind === 'ordered_list');
    assert.equal(actual.lists.length, expectedLists.length, 'native ordered list container count');
    actual.lists.forEach((list, i) => {
      assert.equal(list.start, 1); assert.equal(list.type, 'decimal');
      assert.equal(list.items.length, expectedLists[i].items.length, 'list item count');
      list.items.forEach((item, j) => {
        assert.equal(item.tag, 'LI'); assert.equal(item.display, 'list-item');
        assert(item.value === null || Number(item.value) === j + 1, 'explicit ordered marker');
        assert.equal(item.codes.length, 1);
        assert.equal(item.codes[0].text.replace(/\n$/, ''), expectedLists[i].items[j].text);
        assert.deepEqual(item.links.map(({ text, target }) => ({ text, target })), expectedLists[i].items[j].links.map(expectedLink));
      });
    });
    const expectedLinks = blocks.flatMap(linksOf);
    assert.deepEqual(actual.links.map(({ text, target }) => ({ text, target })), expectedLinks.map(expectedLink), 'actual internal link targets and labels');
    assert.equal(Boolean(actual.svg), hasDiagram);
    assert.equal(actual.mermaidError, null);
    assert.deepEqual(actual.unexpectedActiveTags, []);
    if (hasDiagram) {
      const expectedNodes = byId(input.expected.nodes).map((step, i) => ({
        id: 'n' + i, stepId: step.id, kind: step.kind,
        text: step.kind === 'decision' ? step.label + '：' + step.criterion : step.label
      }));
      const nodeIds = new Map(expectedNodes.map(n => [n.stepId, n.id]));
      assert.deepEqual(actual.nodes.map(n => ({ id: /^flowchart-(n\d+)-\d+$/.exec(n.id)?.[1], kind: n.shape, text: n.text })), expectedNodes.map(({ id, kind, text }) => ({ id, kind, text })), 'rendered nodes from independent source expectations');
      actual.nodes.forEach((n, i) => assert.equal(n.innerText, expectedNodes[i].text, 'literal visible diagram label'));
      const meaning = edge => ({ sequence: '随后', causes: '导致', promotes: '+：促进', inhibits: '−：抑制' }[edge.kind] ??
        (edge.kind === 'branch' ? '判据为' + (edge.branch_outcome ? '真' : '假') + '：' + input.expected.nodes.find(n => n.id === edge.left).criterion : assert.fail('unsupported independent relation')));
      const expectedEdges = byId(input.expected.edges);
      assert.equal(actual.edges.length, expectedEdges.length);
      assert.deepEqual(actual.edgeLabels.map(e => e.text), expectedEdges.map(meaning), 'actual conditions and signed edge labels');
      assert.deepEqual(actual.edgeLabels.map(e => e.innerText), expectedEdges.map(meaning), 'visible conditions and signs');
      record.edgeDirections = actual.edges.map((edge, i) => {
        const source = expectedEdges[i];
        assert(edge.classes.split(/\s+/u).includes('LS-' + nodeIds.get(source.left)));
        assert(edge.classes.split(/\s+/u).includes('LE-' + nodeIds.get(source.right)));
        assert(edge.markerEnd, 'actual end arrow marker');
        assert(edge.end.y > edge.start.y, 'TD visual arrow direction');
        return { relationId: source.id, source: source.left, target: source.right, markerEnd: edge.markerEnd, positiveVerticalDirection: true };
      });
    }
    record.multilineGeometry = [];
    for (const step of input.expected.nodes.filter(step => /[\n\t]/u.test(step.label))) {
      for (const [i, code] of actual.codeBlocks.entries()) {
        const start = code.text.indexOf(step.label);
        if (start < 0 || !code.glyphRanges.length) continue;
        const labelGlyphs = code.glyphRanges.filter(g => g.offset >= start && g.offset < start + step.label.length);
        const tab = labelGlyphs.find(g => g.character === '\t');
        if (step.label.includes('\t')) assert(tab?.width > 0, 'tab occupies visible preformatted width');
        const line = labelGlyphs.findIndex(g => g.character === '\n');
        if (line >= 0) assert(labelGlyphs[line + 1].y > labelGlyphs[line - 1].y, 'literal label LF creates a real new line');
        record.multilineGeometry.push({ stepId: step.id, codeIndex: i, glyphs: labelGlyphs, tabHasWidth: Boolean(tab?.width), realLineBreak: line >= 0 });
      }
    }
    for (let i = 0; i < note.sections.length; i++) {
      const heading = actual.headings.find(h => h.text === note.sections[i].heading);
      await scrollTo(heading.logicalTop - 50);
      const image = 'note-' + index + '-section-' + i + '.png';
      await captureScreenshot(image);
      record.sectionScreenshots.push({ heading: note.sections[i].heading, screenshot: image });
    }
    record.status = 'passed';
    // Every graph/detail/fallback link receives a real click, including return.
    for (const [i, link] of expectedLinks.entries()) {
      await open(note.output_path);
      await scrollTo(actual.links[i].logicalTop - 100);
      const domIndex = await obs.locator('.workspace-leaf.mod-active .markdown-preview-view a.internal-link').evaluateAll((links, wanted) => {
        const view = document.querySelector('.workspace-leaf.mod-active .markdown-preview-view'); const top = view.getBoundingClientRect().y;
        return links.findIndex(link => link.getAttribute('data-href') === wanted.target && Math.abs(link.getBoundingClientRect().y - top + view.scrollTop - wanted.logicalTop) < 2);
      }, actual.links[i]);
      assert(domIndex >= 0, 'actual source link present at its recorded document position');
      await obs.locator('.workspace-leaf.mod-active .markdown-preview-view a.internal-link').nth(domIndex).click();
      await obs.mouse.move(30, 1100);
      await obs.waitForFunction(p => window.app.workspace.getActiveFile()?.path === p, link.target_path);
      // Obsidian applies fragment scrolling after the destination file becomes
      // active, including an animated scroll. Observe its eventual visible
      // heading; do not perform our own corrective scroll here. The one CSS
      // pixel tolerance covers fractional layout/scroll rounding, not a hidden heading.
      await obs.waitForFunction(heading => {
        const view = document.querySelector('.workspace-leaf.mod-active .markdown-preview-view');
        const target = Array.from(view?.querySelectorAll('h2') ?? []).find(h => h.innerText === heading);
        if (!target) return false;
        const v = view.getBoundingClientRect(); const h = target.getBoundingClientRect();
        return h.y >= v.y - 1 && h.bottom <= v.bottom + 1;
      }, link.target_heading, { timeout: 5000 }).catch(() => {});
      const target = await read();
      assert.equal(target.activeFile, link.target_path);
      assert(target.headings.some(h => h.text === link.target_heading));
      const targetHeading = target.headings.find(h => h.text === link.target_heading).rect;
      const fragmentVisible = targetHeading.y >= target.previewRect.y - 1 && targetHeading.y + targetHeading.height <= target.previewRect.y + target.previewRect.height + 1;
      const clickScreenshot = 'note-' + index + '-click-' + i + '.png';
      await captureScreenshot(clickScreenshot);
      result.clicks.push({ source: note.output_path, linkIndex: i, target: target.activeFile, heading: link.target_heading, actualLocatorClick: true,
        targetHeadingRect: targetHeading, previewRect: target.previewRect, fragmentVisible, subpixelToleranceCssPixels: 1, screenshot: clickScreenshot });
      assert(fragmentVisible, 'actual fragment heading scrolled into visible preview');
    }
  }
  if (input.case_name === 'multiline_labels') assert(result.rendered.every(r => r.multilineGeometry.length > 0), 'multiline fallback inspected in overview and details');
  result.mermaidRuntime = await obs.evaluate(() => ({ exposedVersion: window.mermaid?.version ?? null, flowchartConfig: window.mermaid?.mermaidAPI?.getConfig?.()?.flowchart ?? null }));
  result.status = 'passed';
} catch (error) {
  result.status = 'failed'; result.error = { message: String(error), stack: error.stack }; process.exitCode = 1;
} finally {
  result.scope = 'Actual generated product Markdown in isolated installed Obsidian; final native source-byte linkage recorded separately. Knowledge remains pending, scientific_quality not_run, published false.';
  result.finishedUtc = new Date().toISOString();
  fs.writeFileSync(path.join(work, 'render-result.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  if (owned) { await obs.evaluate(() => setTimeout(() => require('@electron/remote').app.quit(), 100)); await new Promise(resolve => setTimeout(resolve, 300)); }
  await browser.close();
}
console.log(JSON.stringify({ status: result.status, case: input.case_name, notes: result.rendered.length, clicks: result.clicks.length }));
