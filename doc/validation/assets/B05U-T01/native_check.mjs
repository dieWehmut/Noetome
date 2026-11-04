// Bounded end-to-end checks of the installed Noetome WebView, not desktop input.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';

const options = Object.fromEntries(Array.from({length:(process.argv.length-2)/2},(_,i)=>[process.argv[2+i*2],process.argv[3+i*2]]));
const require = createRequire(import.meta.url);
const {chromium} = require(path.join(options['--modules'],'playwright'));
const report = {stage:options['--stage'],endpoint:options['--endpoint'],checks:[],errors:[]};
const output = options['--output'];
async function check(name, action) { try { const detail=await action(); report.checks.push({name,passed:true,detail}); return detail; } catch(error) {report.checks.push({name,passed:false,error:String(error)});throw error;} }
try {
  const browser = await chromium.connectOverCDP(options['--endpoint'],{timeout:15000});
  const pages = browser.contexts().flatMap(context=>context.pages()).filter(page=>page.url()==='http://tauri.localhost/');
  assert.equal(pages.length,1,'Expected only the installed Noetome page');
  const page=pages[0];
  page.on('pageerror', error=>report.errors.push(String(error)));
  page.on('console', message=>{if(message.type()==='error')report.errors.push(message.text());});
  await page.locator('.app-layout').waitFor();
  await page.waitForFunction(()=>['light','dark'].includes(document.documentElement.dataset.theme));
  report.viewport=await page.evaluate(()=>({width:innerWidth,height:innerHeight,dpr:devicePixelRatio}));
  report.tauri=await page.evaluate(()=>typeof window.__TAURI_INTERNALS__?.invoke==='function');
  assert.equal(report.tauri,true);
  const gear=()=>page.getByRole('button',{name:'设置',exact:true}).click();
  const category=label=>page.getByRole('navigation',{name:'设置分类'}).getByRole('button',{name:label,exact:true}).click();
  const readClose=async()=>{await gear();await category('窗口行为');await page.waitForFunction(()=>!document.getElementById('close-behavior').disabled);return await page.getByLabel('关闭窗口时',{exact:true}).inputValue();};
  const saveClose=async value=>{const field=page.getByLabel('关闭窗口时',{exact:true});if(await field.inputValue()!==value){await field.selectOption(value);await page.getByRole('button',{name:'保存设置',exact:true}).click();await page.getByText(`已保存：${value==='exit'?'直接退出':'隐藏到托盘'}。`,{exact:true}).waitFor();}assert.equal(await field.inputValue(),value);};
  if(options['--stage']==='first') {
    report.originalTheme=await page.locator('html').getAttribute('data-theme');
    report.originalClose=await check('Native window preference loads from Rust',readClose);
    assert.ok(['hide_to_tray','exit'].includes(report.originalClose));
    await check('Native runtime, CLI help and version',async()=>{
      await page.locator('.rail-top').getByRole('button',{name:'运行检查',exact:true}).click();
      await page.getByRole('button',{name:'检查连接',exact:true}).click();
      await page.getByText('连接正常',{exact:true}).waitFor();
      const fields=await page.locator('.runtime-details dd').allTextContents();assert.equal(fields[0],'0.1.0');assert.equal(fields[1],'0.1.0');
      await page.getByRole('button',{name:'查看帮助',exact:true}).click();await page.locator('.console-status.success').waitFor();
      const help=await page.locator('pre[aria-label="CLI 运行结果"]').textContent();assert.ok(help.includes('--help')&&help.includes('source'));
      await page.getByRole('button',{name:'查看版本',exact:true}).click();await page.locator('.console-status.success').waitFor();
      const version=await page.locator('pre[aria-label="CLI 运行结果"]').textContent();assert.ok(version.includes('0.1.0'));
      return {fields,help,version};
    });
    await check('Native close preference saves and rereads',async()=>{
      await readClose();const alternate=report.originalClose==='exit'?'hide_to_tray':'exit';await saveClose(alternate);
      await category('外观');const reread=await readClose();assert.equal(reread,alternate);await saveClose('exit');return {original:report.originalClose,alternate,reread,restartClose:'exit'};
    });
    await check('Native Moon and Sun directly save dark theme',async()=>{
      await gear();if(await page.locator('html').getAttribute('data-theme')==='dark')await page.getByRole('button',{name:'切换到亮色',exact:true}).click();
      await page.getByRole('button',{name:'切换到暗色',exact:true}).click();
      await page.getByText('主题已保存，下次打开时继续使用。',{exact:true}).waitFor();
      assert.equal(await page.getByLabel('主题',{exact:true}).inputValue(),'dark');await page.getByRole('button',{name:'切换到亮色',exact:true}).waitFor();
      await page.getByRole('heading',{name:'外观',exact:true}).click();await page.screenshot({path:options['--dark-image']});return {theme:'dark',control:'切换到亮色'};
    });
    assert.equal(report.errors.length,0,JSON.stringify(report.errors));
    report.requestedExit=true;
    await page.getByRole('button',{name:'关闭窗口',exact:true}).click().catch(error=>{if(!/closed|destroyed/i.test(String(error)))throw error;});
  } else {
    await check('Dark theme survives installed process restart',async()=>{assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');await gear();assert.equal(await page.getByLabel('主题',{exact:true}).inputValue(),'dark');return {theme:'dark',freshProcess:true};});
    await check('Exit preference survives restart and original value is restored',async()=>{assert.equal(await readClose(),'exit');await saveClose(options['--original-close']);await category('外观');const restored=await readClose();assert.equal(restored,options['--original-close']);return {restarted:'exit',restored};});
    await check('Original theme restores and survives reload',async()=>{await gear();await page.getByLabel('主题',{exact:true}).selectOption(options['--original-theme']);await page.reload();await page.locator('.app-layout').waitFor();await page.waitForFunction(value=>document.documentElement.dataset.theme===value,options['--original-theme']);await gear();assert.equal(await page.getByLabel('主题',{exact:true}).inputValue(),options['--original-theme']);await page.getByRole('heading',{name:'外观',exact:true}).click();await page.screenshot({path:options['--light-image']});return {restored:options['--original-theme'],reload:true};});
    assert.equal(report.errors.length,0,JSON.stringify(report.errors));
  }
  report.passed=true;
  await fs.writeFile(output,JSON.stringify(report,null,2)+'\n');
  process.exit(0); // Disconnect this test client; do not close the WebView browser.
} catch(error) {report.passed=false;report.failure=String(error);await fs.writeFile(output,JSON.stringify(report,null,2)+'\n');process.exit(1);}
