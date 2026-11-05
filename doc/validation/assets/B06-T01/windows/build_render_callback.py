import argparse,json
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('work',type=Path)
work=parser.parse_args().work
value=json.loads((work/'visual-input.json').read_text(encoding='utf-8'))
template=r'''async (page) => {
  const input = __VISUAL_INPUT__;
  const browser = await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:'+input.port);
  const obs = browser.contexts()[0].pages()[0];
  await obs.waitForFunction(() => window.app?.vault?.adapter?.basePath, {timeout:30000});
  const identity = await obs.evaluate(() => ({
    userData:require('@electron/remote').app.getPath('userData'),
    version:require('@electron/remote').app.getVersion(),
    vaultPath:window.app.vault.adapter.basePath,
    plugins:Object.keys(window.app.plugins.plugins),
    electron:window.process.versions.electron,chrome:window.process.versions.chrome,
    title:document.title
  }));
  const normalizePath = s => s.replace(/\\+/g,'/').toLowerCase();
  if(normalizePath(identity.userData)!==normalizePath(input.profile) || normalizePath(identity.vaultPath)!==normalizePath(input.vault) || identity.plugins.length!==0) throw Error('Own isolated profile/Vault/plugin identity mismatch');
  if(identity.version!=='1.6.7') throw Error('Unexpected installed actual application version; inspect identity');
  await obs.setViewportSize({width:1600,height:1200});
  const normalizeText = s => s.replace(/\r/g,'').replace(/\s+/gu,' ').trim();
  const equal = (a,b,label) => {if(JSON.stringify(a)!==JSON.stringify(b)) throw Error(label+' mismatch: '+JSON.stringify({actual:a,expected:b}));};
  const payload = x => normalizeText(x.text+(x.links.length?' '+x.links.map(l=>l.display_text).join(' '):''));
  const open = async path => {
    await obs.evaluate(async filePath => {
      const file=window.app.vault.getAbstractFileByPath(filePath);
      if(!file)throw Error('Selected own Vault note missing '+filePath);
      await window.app.workspace.getLeaf(false).openFile(file,{active:true,state:{mode:'preview'}});
    },path);
    await obs.waitForFunction(filePath => window.app.workspace.getActiveFile()?.path===filePath && window.app.workspace.activeLeaf?.view?.getState()?.mode==='preview',path);
    await obs.locator('.markdown-preview-view').waitFor({state:'visible'});
    await obs.waitForTimeout(150);
  };
  const read = async () => obs.evaluate(() => {
    const view=document.querySelector('.markdown-preview-view');
    return {activeFile:window.app.workspace.getActiveFile()?.path,activeView:window.app.workspace.activeLeaf?.view?.getState(),
      dom:view.outerHTML,headings:Array.from(view.querySelectorAll('h1,h2')).map(h=>({tag:h.tagName,text:h.innerText,dataHeading:h.getAttribute('data-heading')})),
      tables:Array.from(view.querySelectorAll('table')).map(t=>Array.from(t.querySelectorAll('tr')).map(r=>Array.from(r.children).map(c=>c.innerText))),
      paragraphs:Array.from(view.querySelectorAll('p')).map(p=>p.innerText),
      links:Array.from(view.querySelectorAll('a.internal-link')).map(a=>({text:a.innerText,dataHref:a.getAttribute('data-href'),href:a.getAttribute('href'),className:a.className})),
      brCount:view.querySelectorAll('br').length,bodyText:view.innerText};
  });
  const rendered=[];const clicks=[];
  for(let noteIndex=0;noteIndex<input.plan.notes.length;noteIndex++) {
    const note=input.plan.notes[noteIndex];await open(note.output_path);
    const actual=await read();
    equal(actual.activeFile,note.output_path,'active native note');
    equal(actual.headings.map(h=>normalizeText(h.text)),[normalizeText(note.title),normalizeText(note.heading)],'actual note headings');
    const tables=note.nodes.filter(n=>n.kind==='table');
    const expectedTables=tables.map(t=>[t.columns.map(c=>normalizeText(c.label)),...t.rows.map(r=>r.cells.map(payload))]);
    equal(actual.tables.map(t=>t.map(r=>r.map(normalizeText))),expectedTables,'actual native tables');
    const paragraphs=note.nodes.filter(n=>n.kind==='paragraph');
    equal(actual.paragraphs.map(normalizeText),paragraphs.map(payload),'actual native paragraphs');
    const expectedLinks=note.nodes.flatMap(n=>n.kind==='table'?n.rows.flatMap(r=>r.cells.flatMap(c=>c.links)):n.links);
    equal(actual.links.map(l=>({text:normalizeText(l.text),target:l.dataHref})),expectedLinks.map(l=>({text:normalizeText(l.display_text),target:l.target_path.slice(0,-3)+'#'+l.target_heading})),'actual native internal links');
    const screenshot=input.wsl_work+'/note-'+noteIndex+'.png';
    await obs.screenshot({path:screenshot,fullPage:true});
    rendered.push({note:note.output_path,actual,screenshot,checks:{headings:true,tableCells:true,paragraphs:true,linkTargets:true}});
    if(noteIndex===0) {
      for(let linkIndex=0;linkIndex<expectedLinks.length;linkIndex++) {
        const link=expectedLinks[linkIndex];await open(note.output_path);
        const target=link.target_path.slice(0,-3)+'#'+link.target_heading;
        await obs.locator('.markdown-preview-view a.internal-link').filter({hasText:link.display_text}).nth(linkIndex).click();
        await obs.waitForFunction(path => window.app.workspace.getActiveFile()?.path===path,link.target_path);
        await obs.locator('.markdown-preview-view').waitFor({state:'visible'});
        await obs.waitForTimeout(100);
        const after=await read();
        equal(after.activeFile,link.target_path,'actual clicked note path');
        if(!after.headings.some(h=>normalizeText(h.text)===normalizeText(link.target_heading)))throw Error('Clicked heading absent');
        const clickScreenshot=input.wsl_work+'/click-'+linkIndex+'.png';
        await obs.screenshot({path:clickScreenshot,fullPage:true});
        clicks.push({source:note.output_path,linkIndex,plannedTarget:target,actualTarget:after.activeFile,heading:link.target_heading,targetDom:after.dom,headings:after.headings,screenshot:clickScreenshot,actualLocatorClick:true});
      }
    }
  }
  return {status:'passed',caseName:input.case_name,snapshotHead:input.snapshot_head,identity,rendered,clicks,
    knowledgeStatus:input.plan.knowledge_status,scientificQuality:input.plan.scientific_quality,published:input.plan.published,
    scope:'Actual installed Obsidian 1.6.7 native DOM and locator clicks on a fresh owned profile/Vault; no version compatibility matrix'};
}'''
callback=template.replace('__VISUAL_INPUT__',json.dumps(value,ensure_ascii=False))
destination=work/'actual-render.js'
assert not destination.exists(),'never overwrite actual executed callback'
destination.write_text(callback,encoding='utf-8')
print(destination)
