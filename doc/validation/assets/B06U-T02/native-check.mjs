import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import net from 'node:net';
const require=createRequire(import.meta.url);
const {chromium}=require('C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const output=path.dirname(new URL(import.meta.url).pathname.replace(/^\//,''));
const installed=path.join(process.env.LOCALAPPDATA,'Programs/Noetome/noetome-desktop.exe');
const source='D:/Noetome/target/b06u-t02-source-20261105';
const sha=data=>createHash('sha256').update(data).digest('hex');
const receipt=JSON.parse((await fs.readFile(path.join(source,'.artifacts/desktop-install.json'),'utf8')).replace(/^\uFEFF/,''));
const report={checks:[],errors:[],installed,installedSha256:sha(await fs.readFile(installed)),diagnostics:'process-only loopback; no product or global environment change'};
assert.equal(report.installedSha256,receipt.executable_sha256);
assert.equal(receipt.matches_current_build,true);
const probe=net.createServer();
await new Promise(resolve=>probe.listen(0,'127.0.0.1',resolve));
const port=probe.address().port;
await new Promise(resolve=>probe.close(resolve));
const environment={...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-address=127.0.0.1 --remote-debugging-port=${port}`};
const owned=spawn(installed,[],{env:environment,stdio:'ignore'});
report.diagnosticPid=owned.pid;
const endpoint=`http://127.0.0.1:${port}`;
async function check(name,action){try{const detail=await action();report.checks.push({name,passed:true,detail});}catch(e){report.checks.push({name,passed:false,error:String(e)});throw e;}}
try {
 let ready=false;
 for(let i=0;i<80;i++){try{const targets=await (await fetch(endpoint+'/json/list')).json();if(targets.some(t=>t.url==='http://tauri.localhost/')){ready=true;break;}}catch{} await new Promise(r=>setTimeout(r,200));}
 assert.ok(ready,'installed WebView endpoint');
 const browser=await chromium.connectOverCDP(endpoint);
 report.browserVersion=browser.version();
 const pages=browser.contexts().flatMap(c=>c.pages()).filter(p=>p.url()==='http://tauri.localhost/');
 assert.equal(pages.length,1);
 const page=pages[0];
 page.on('pageerror',e=>report.errors.push(String(e)));
 page.on('console',m=>{if(m.type()==='error')report.errors.push(m.text());});
 await page.locator('.app-layout').waitFor();
 assert.equal(await page.evaluate(()=>typeof window.__TAURI_INTERNALS__?.invoke==='function'),true);
 report.viewport=await page.evaluate(()=>({width:innerWidth,height:innerHeight,dpr:devicePixelRatio}));
 const side=page.locator('#app-sidebar'), rail=page.getByRole('navigation',{name:'快捷导航'});
 const gear=()=>rail.getByRole('button',{name:'设置',exact:true}).click();
 const home=()=>rail.getByRole('button',{name:'工作台',exact:true}).click();
 const category=name=>page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name,exact:true}).click();
 await check('Installed script and stylesheet equal the final build',async()=>{
   const client=await page.context().newCDPSession(page);
   await client.send('Page.enable');
   const {frameTree}=await client.send('Page.getResourceTree');
   const assets=frameTree.resources.filter(r=>['Script','Stylesheet'].includes(r.type));
   const results=[];
   for(const asset of assets){
     const body=await client.send('Page.getResourceContent',{frameId:frameTree.frame.id,url:asset.url});
     const relative=new URL(asset.url).pathname.replace(/^\//,'');
     const bytes=Buffer.from(body.content,body.base64Encoded?'base64':'utf8');
     assert.deepEqual(bytes,await fs.readFile(path.join(source,'apps/desktop/dist',relative)));
     results.push({relative,bytes:bytes.length,sha256:sha(bytes)});
   }
   await client.detach();
   assert.ok(results.length>=2);return results;
 });
 await check('Settings hide the sidebar and reclaim its width',async()=>{
   assert.equal(await side.isVisible(),true);const before=await page.locator('.main-layout').boundingBox();await gear();assert.equal(await side.isVisible(),false);
   const after=await page.locator('.main-layout').boundingBox();assert.ok(after.width-before.width>=200);assert.equal(await rail.isVisible(),true);
   assert.equal(await page.getByRole('button',{name:'切换侧栏',exact:true}).isDisabled(),true);
   for(const name of ['窗口行为','关于 Noetome','外观']){await category(name);assert.equal(await side.isVisible(),false);}
   await page.getByRole('menuitem',{name:'View',exact:true}).click();
   assert.equal(await page.getByRole('menuitemcheckbox',{name:'显示侧栏',exact:true}).isDisabled(),true);
   await page.keyboard.press('Escape');
   return {reclaimedWidth:after.width-before.width,categories:3,railVisible:true};
 });
 await check('Return and history restore expanded sidebar',async()=>{
   await home();assert.equal(await side.isVisible(),true);await gear();await page.getByRole('button',{name:'后退',exact:true}).click();assert.equal(await side.isVisible(),true);
   await page.getByRole('button',{name:'前进',exact:true}).click();assert.equal(await side.isVisible(),false);return true;
 });
 await check('Collapsed preference survives settings navigation',async()=>{
   await home();await page.getByRole('button',{name:'切换侧栏',exact:true}).click();assert.equal(await side.isVisible(),false);
   await gear();await home();assert.equal(await side.isVisible(),false);await page.getByRole('button',{name:'切换侧栏',exact:true}).click();return true;
 });
 await check('Installed runtime calls real Rust and CLI',async()=>{
   await rail.getByRole('button',{name:'运行检查',exact:true}).click();await page.getByRole('button',{name:'检查连接',exact:true}).click();await page.getByText('连接正常',{exact:true}).waitFor();
   const fields=await page.locator('.runtime-details dd').allTextContents();assert.equal(fields[0],'0.1.0');assert.equal(fields[1],'0.1.0');
   await page.getByRole('button',{name:'查看版本',exact:true}).click();await page.locator('.console-status.success').waitFor();
   const version=await page.locator('pre[aria-label="CLI 运行结果"]').textContent();assert.ok(version.includes('0.1.0'));
   return {fields,version};
 });
 await check('Window preference loads through installed Rust',async()=>{
   await gear();await category('窗口行为');await page.waitForFunction(()=>!document.getElementById('close-behavior').disabled);
   const value=await page.getByLabel('关闭窗口时',{exact:true}).inputValue();assert.ok(['hide_to_tray','exit'].includes(value));return {value,modified:false};
 });
 await gear();await page.getByRole('heading',{name:'外观',exact:true}).click();
 await page.screenshot({path:path.join(output,'installed-settings.png')});
 assert.equal(report.errors.length,0,JSON.stringify(report.errors));
 report.passed=true;
} catch(error){report.passed=false;report.failure=String(error);}
finally {
 const exitPromise=new Promise(resolve=>owned.once('exit',(code,signal)=>resolve({code,signal})));
 owned.kill();report.diagnosticCleanup=await exitPromise;
 const ordinaryEnvironment={...process.env};delete ordinaryEnvironment.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;
 const ordinary=spawn(installed,[],{env:ordinaryEnvironment,stdio:'ignore',detached:true});
 await new Promise(r=>setTimeout(r,1500));
 report.ordinaryLaunch={pid:ordinary.pid,running:ordinary.exitCode===null,diagnosticsEnabled:false};
 ordinary.unref();
 if(!report.ordinaryLaunch.running) report.passed=false;
 await fs.writeFile(path.join(output,'native-verification.json'),JSON.stringify(report,null,2)+'\n');
}
process.exit(report.passed?0:1);
