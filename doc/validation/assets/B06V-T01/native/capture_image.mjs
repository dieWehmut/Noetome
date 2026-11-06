// Actual installed Obsidian image, attachment and SVG decoding evidence.
// SVG img attachments have no child DOM. Observe native loaded pixels, then
// obtain rendered vector geometry for the exact same bytes in an auxiliary
// page of the same owned Chromium and require identical decoded RGBA pixels.
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
const verifySourceSvg = (semantic, geometry) => {
  const s = semantic.source;
  const byId = Object.fromEntries(geometry.renderedElements.map(e => [e.id,e]));
  const close = (a,b) => assert(Math.abs(a-b) < .001, 'actual rendered geometry equals independently authored coordinate');
  const contains = (a,b) => a.x <= b.x && a.y <= b.y && a.x+a.width >= b.x+b.width && a.y+a.height >= b.y+b.height;
  const sourceBox = value => ({x:value[0],y:value[1],width:value[2],height:value[3]});
  const requiredIds = [];
  const labelContrasts=[];
  const luminance=hex=>{const rgb=hex.replace('#','').match(/../g).map(v=>parseInt(v,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return .2126*rgb[0]+.7152*rgb[1]+.0722*rgb[2];};
  const node = id => { requiredIds.push(id); const e=byId[id]; assert(e,'rendered independently expected field: '+id); return e; };
  const label = (id,value,bounds,background='#ffffff') => {
    const e=node(id);
    assert.equal(e.tag,'text'); assert.equal(e.text,value,'actual in-image literal source label/unit/effect/legend');
    assert(e.bbox.width > 0 && e.bbox.height > 0 && e.style.display !== 'none' && e.style.visibility === 'visible' && Number(e.style.opacity)>0,'actual expected label is rendered with area');
    assert.equal(e.style.fill,'rgb(0, 0, 0)','actual source label ink remains black');
    const ratio=(luminance(background)+.05)/.05;
    assert(ratio>=4.5,'actual black label contrasts with its preserved opaque background');
    labelContrasts.push({id,actualInk:e.style.fill,actualBackground:background,contrastRatio:ratio});
    assert(contains(geometry.root.bbox,e.bbox),'actual label fits whole selected image');
    // SVG getBBox includes the font's nominal ascent/descent, even for a
    // small '+' glyph. Permit a one-pixel font-metric margin at source boxes.
    if(bounds) assert(e.bbox.x>=bounds.x-1&&e.bbox.y>=bounds.y-1&&e.bbox.x+e.bbox.width<=bounds.x+bounds.width+1&&e.bbox.y+e.bbox.height<=bounds.y+bounds.height+1,'actual label lies in its source-associated shape/legend within one nominal font-metric pixel');
    return e;
  };
  const shape = (id,kind,value) => {
    const e=node(id); assert.equal(e.tag,kind); const b=sourceBox(value);
    for(const k of ['x','y','width','height']) close(e.bbox[k],b[k]);
    assert(e.rect.width > 0 && e.rect.height > 0,'actual source-mapped shape has rendered area');
    return e;
  };
  assert.deepEqual(geometry.root.bbox,{x:0,y:0,width:semantic.dimensions[0],height:semantic.dimensions[1]},'actual SVG view preserves original whole-image coordinate system');
  const canvas=shape('image-canvas','rect',[0,0,...semantic.dimensions]);
  assert.equal(canvas.attributes.fill,'#ffffff','actual original white canvas remains behind every source-mapped element');
  const legends=Object.fromEntries(s.legend.map(l => [l.id,l]));
  for(const e of s.elements) {
    const actualShape=shape('element-'+e.id,e.kind,e.bbox);
    const background=e.legend_id===null?'#ffffff':legends[e.legend_id].color;
    assert.equal(actualShape.attributes.fill,background,'actual element retains independently selected source fill');
    label('element-'+e.id+'-label',e.label,sourceBox(e.bbox),background);
    if(e.unit!==null) label('element-'+e.id+'-unit',e.unit,sourceBox(e.bbox),background);
  }
  for(const l of s.legend) {
    const e=shape('legend-'+l.id,'rect',l.bbox);
    assert.equal(e.attributes.fill,l.color,'actual legend swatch retains original declared color');
    label('legend-'+l.id+'-label',l.label,sourceBox(l.bbox),l.color);
    label('legend-'+l.id+'-meaning',l.meaning,sourceBox(l.bbox),l.color);
  }
  for(const r of s.relations) {
    const e=node('relation-'+r.id); assert.equal(e.tag,'line');
    for(const [key,value] of Object.entries({x1:r.start[0],y1:r.start[1],x2:r.end[0],y2:r.end[1]})) close(Number(e.attributes[key]),value);
    assert.equal(e.attributes.stroke,'#000000','actual source PNG relation segment is black; source legend association is retained by effect and legend fields');
    if(r.direction==='forward'||r.direction==='reverse') {
      const head=node('relation-'+r.id+'-head'); assert.equal(head.tag,'polygon');
      const p=head.attributes.points.split(' ').map(pair=>pair.split(',').map(Number)); assert.equal(p.length,3);
      const tip=r.direction==='forward'?r.end:r.start, base=r.direction==='forward'?r.start:r.end;
      close(p[0][0],tip[0]);close(p[0][1],tip[1]);
      const dx=tip[0]-base[0],dy=tip[1]-base[1];
      const dot=q=>(tip[0]-q[0])*dx+(tip[1]-q[1])*dy;
      const cross=q=>(q[0]-tip[0])*dy-(q[1]-tip[1])*dx;
      assert(dot(p[1])>0&&dot(p[2])>0&&cross(p[1])*cross(p[2])<0,'actual arrow tip is at independently selected directed endpoint and symmetric wings lie behind it');
      assert(head.bbox.width>0&&head.bbox.height>0,'actual directed arrowhead has visible area');
      assert.equal(head.attributes.fill,e.attributes.stroke,'actual arrow and segment belong to same source legend');
    } else assert(!byId['relation-'+r.id+'-head'],'undirected relation has no invented arrowhead');
    if(r.effect==='positive'||r.effect==='negative') label('relation-'+r.id+'-effect',r.effect==='positive'?'+':'−');
    label('relation-'+r.id+'-label',r.label);
    if(r.unit!==null) label('relation-'+r.id+'-unit',r.unit);
  }
  for(const r of s.spatial_relations) {
    const a=byId['element-'+r.a].bbox,b=byId['element-'+r.b].bbox;
    if(r.relation==='left_of') assert(a.x+a.width<=b.x,'actual shape left of independently selected reference');
    else if(r.relation==='above') assert(a.y+a.height<=b.y,'actual shape above independently selected reference');
    else if(r.relation==='inside') assert(contains(b,a),'actual shape inside independently selected reference');
    else if(r.relation==='overlaps') assert(Math.max(a.x,b.x)<Math.min(a.x+a.width,b.x+b.width)&&Math.max(a.y,b.y)<Math.min(a.y+a.height,b.y+b.height),'actual source shapes overlap');
    else assert.fail('unknown spatial semantics must not reach native SVG');
  }
  assert.deepEqual(geometry.renderedElements.map(e=>e.id).sort(),requiredIds.sort(),'no extra or missing semantic SVG primitives');
  return {status:'passed',sourceElementCount:s.elements.length,sourceRelationCount:s.relations.length,sourceLegendCount:s.legend.length,sourceSpatialCount:s.spatial_relations.length,
    contrastReview:{method:'actual black SVG ink against asserted opaque white canvas or preserved source shape/legend fill; relative sRGB luminance',minimumContrastRatio:Math.min(...labelContrasts.map(e=>e.contrastRatio)),labels:labelContrasts}};
};
let owned = false;
try {
  await obs.waitForFunction(() => window.app?.workspace?.layoutReady && window.app.workspace.rootSplit, { timeout: 30000 });
  result.identity = await obs.evaluate(() => ({
    appVersion: require('@electron/remote').app.getVersion(),
    userData: require('@electron/remote').app.getPath('userData'),
    vaultPath: window.app.vault.adapter.basePath,
    communityPlugins: Object.keys(window.app.plugins.plugins),
      theme: window.app.vault.getConfig('theme'),
      bodyClass: document.body.className,
      bodyBackground: getComputedStyle(document.body).backgroundColor,
    electron: process.versions.electron, chromium: process.versions.chrome
  }));
  assert.equal(norm(result.identity.userData), norm(input.profile));
  assert.equal(norm(result.identity.vaultPath), norm(input.vault));
  owned = true;
  assert.equal(result.identity.appVersion, '1.6.7');
  assert.deepEqual(result.identity.communityPlugins, []);
    if(input.spec.theme === 'dark') assert(result.identity.bodyClass.includes('theme-dark'),'actual requested dark theme applied');
    else if(input.spec.theme === 'light') assert(result.identity.bodyClass.includes('theme-light'),'actual requested light theme applied');
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
      previewRect: rect(view), previewBackground: getComputedStyle(view).backgroundColor, text: view.innerText,
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
    if (note.svg_semantics) {
      const semantic = note.svg_semantics;
      const image = actual.images.find(i => decodeURI(i.src).includes(semantic.file));
      assert(image, 'native preview contains the selected SVG attachment');
      await scrollTo(image.logicalTop - 100);
      const native = await obs.evaluate(file => {
        const e = Array.from(document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view img')).find(e => decodeURI(e.src).includes(file));
        if (!e || !e.complete || !e.naturalWidth) throw new Error('native SVG image not loaded');
        const fs = require('fs'), path = require('path'), crypto = require('crypto');
        const bytes = fs.readFileSync(path.join(window.app.vault.adapter.basePath, file));
        const canvas = document.createElement('canvas');
        canvas.width = e.naturalWidth; canvas.height = e.naturalHeight;
        canvas.getContext('2d').drawImage(e, 0, 0);
        const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
        let transparentPixelCount=0;
        for(let n=3;n<pixels.length;n+=4) if(pixels[n]<255) transparentPixelCount++;
        const view=document.querySelector('.workspace-leaf.mod-active .markdown-preview-view');
        return { src: e.src, complete: e.complete, naturalWidth: e.naturalWidth, naturalHeight: e.naturalHeight,
          svgDataUrl: 'data:image/svg+xml;base64,' + bytes.toString('base64'),
          svgSha256: crypto.createHash('sha256').update(bytes).digest('hex'),
          decodedRgbaSha256: crypto.createHash('sha256').update(pixels).digest('hex'),
          transparentPixelCount, pixelCount:canvas.width*canvas.height,
          cornerRgba:Array.from(pixels.slice(0,4)),
          nativePreviewBackground:getComputedStyle(view).backgroundColor,
          nativeBackgroundPrimary:getComputedStyle(view).getPropertyValue('--background-primary').trim(),
          canvasPng: canvas.toDataURL('image/png'),
          noteHasInlineSvg: !!e.querySelector('svg') };
      }, semantic.file);
      const nativeDecodedPng='note-'+index+'-svg-native-decoded.png';
      const nativePreviewScreenshot='note-'+index+'-svg-preview.png';
      fs.writeFileSync(path.join(work,nativeDecodedPng),Buffer.from(native.canvasPng.split(',')[1],'base64'),{flag:'wx'});
      await obs.bringToFront();
      await obs.screenshot({path:path.join(work,nativePreviewScreenshot),fullPage:true,timeout:60000});
      const {svgDataUrl:earlyData,canvasPng:earlyCanvas,...earlyNative}=native;
      record.svgEvidence={status:'checking',native:earlyNative,nativeDecodedPng,nativePreviewScreenshot,semanticExpected:semantic};
      // Electron CDP rejects Target.createTarget. Create a self-owned temporary
      // BrowserWindow through this already identity-verified test instance.
      const auxiliaryId = await obs.evaluate(url => {
        const {BrowserWindow} = require('@electron/remote');
        const w = new BrowserWindow({width:1000,height:700,show:true,webPreferences:{nodeIntegration:false,contextIsolation:true}});
        w.loadURL(url); return w.id;
      }, native.svgDataUrl);
      let auxiliary;
      try {
        for (let n=0;n<100;n++) {
          auxiliary = browser.contexts()[0].pages().find(p => p.url() === native.svgDataUrl);
          if (auxiliary) break;
          await obs.waitForTimeout(100);
        }
        assert(auxiliary, 'self-owned auxiliary BrowserWindow exposed through same instance CDP');
        await auxiliary.waitForFunction(() => document.querySelector('svg'));
        const geometry = await auxiliary.evaluate(() => {
          const svg = document.querySelector('svg');
          const box = e => { const b = typeof e.getBBox === 'function' ? e.getBBox() : null, r = e.getBoundingClientRect(); return { bbox: b ? {x:b.x,y:b.y,width:b.width,height:b.height} : null, rect: {x:r.x,y:r.y,width:r.width,height:r.height} }; };
          return { documentUrl: location.protocol, root: box(svg),
            renderedLabels: Array.from(svg.querySelectorAll('text')).map(e => ({text:e.textContent,id:e.id,...box(e)})),
            renderedElements: Array.from(svg.querySelectorAll('[id]')).map(e => ({id:e.id,tag:e.localName,text:e.textContent,...box(e),attributes:Object.fromEntries(Array.from(e.attributes).map(a => [a.name,a.value])),style:{fill:getComputedStyle(e).fill,stroke:getComputedStyle(e).stroke,opacity:getComputedStyle(e).opacity,visibility:getComputedStyle(e).visibility,display:getComputedStyle(e).display}})),
            activeElements: Array.from(svg.querySelectorAll('script,foreignObject,image,iframe,object')).map(e => e.outerHTML) };
        });
        record.svgEvidence.geometry=geometry;
        assert.deepEqual(geometry.activeElements, [], 'bounded SVG has no active or external resources');
        if(semantic.source) {
          record.svgOpacityReview={transparentPixelCount:native.transparentPixelCount,cornerRgba:native.cornerRgba,nativePreviewBackground:native.nativePreviewBackground,nativeBackgroundPrimary:native.nativeBackgroundPrimary};
          assert.equal(native.transparentPixelCount,0,'actual final SVG preserves an opaque original-image canvas, including behind black relation labels in dark theme');
          assert.deepEqual(native.cornerRgba,[255,255,255,255],'actual final SVG original white canvas at image corner');
        }
        const labelled = geometry.renderedLabels;
        if (semantic.source) record.svgSourceReview = verifySourceSvg(semantic, geometry);
        else assert.deepEqual(labelled.map(e => e.text), semantic.labels, 'actual vector renderer retains independently expected signs, units and legend');
        labelled.forEach(e => assert(e.bbox.width > 0 && e.bbox.height > 0, 'rendered label has visible area: ' + e.text));
        const byId = Object.fromEntries(geometry.renderedElements.map(e => [e.id, e]));
        for (const relation of semantic.relations ?? []) {
          const a = byId[relation.a]; assert(a, 'source-mapped rendered element');
          if (relation.kind === 'left_of') { const b = byId[relation.b]; assert(a.bbox.x + a.bbox.width <= b.bbox.x, 'actual rendered element left of peer'); }
          else if (relation.kind === 'above') { const b = byId[relation.b]; assert(a.bbox.y + a.bbox.height <= b.bbox.y, 'actual rendered element above peer'); }
          else if (relation.kind === 'arrow_right') {
            assert(Number(a.attributes.x2) > Number(a.attributes.x1), 'actual line points from left to right');
            assert.equal(a.attributes['marker-end'], 'url(#' + relation.marker + ')', 'actual arrow marker at directed line end');
            assert.equal(a.attributes['marker-start'] ?? null, null, 'no unintended arrow at start');
            const marker = byId[relation.marker];
            assert(marker && marker.tag === 'marker' && marker.attributes.orient === 'auto', 'actual marker follows line direction');
            assert(geometry.renderedElements.find(e => e.id === relation.a).bbox.width > 0);
          } else assert.fail('unsupported independent SVG spatial relation: ' + relation.kind);
        }
        const rgba = await auxiliary.evaluate(async dataUrl => {
          const e = new Image(); e.src = dataUrl; await e.decode();
          const canvas = document.createElementNS('http://www.w3.org/1999/xhtml','canvas'); canvas.width = e.naturalWidth; canvas.height = e.naturalHeight;
          canvas.getContext('2d').drawImage(e, 0, 0);
          return Array.from(canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data);
        }, native.svgDataUrl);
        const { createHash } = await import('node:crypto');
        const auxiliaryRgbaSha256 = createHash('sha256').update(Buffer.from(rgba)).digest('hex');
        assert.equal(auxiliaryRgbaSha256, native.decodedRgbaSha256, 'same-version auxiliary SVG rendering decodes exactly the native local SVG img pixels');
        const screenshot = 'note-' + index + '-svg-vector.png';
        // Playwright fullPage screenshot assumes an HTML scrolling root. The
        // auxiliary document is SVG/XML, so capture its actual Electron page.
        const vectorScreenshot = await obs.evaluate(async id => {
          const w = require('@electron/remote').BrowserWindow.fromId(id);
          const image = await w.webContents.capturePage();
          return image.toPNG().toString('base64');
        }, auxiliaryId);
        fs.writeFileSync(path.join(work,screenshot), Buffer.from(vectorScreenshot,'base64'), {flag:'wx'});
        const { svgDataUrl, canvasPng, ...nativeReceipt } = native;
        record.svgEvidence = { method:'native Obsidian external SVG img decoding plus exact-byte auxiliary same-Chromium rendered bbox; equal decoded RGBA hashes',
          native:nativeReceipt, auxiliaryRgbaSha256, geometry, semanticExpected:semantic,
          nativeDecodedPng,nativePreviewScreenshot, auxiliaryScreenshot:screenshot, status:'passed'};
      } finally { await obs.evaluate(id => require('@electron/remote').BrowserWindow.fromId(id)?.close(), auxiliaryId); }
    }
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
