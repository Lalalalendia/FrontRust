import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { chromium, firefox } from 'playwright';

const BASE = process.env.FRONT_RUST_BASE_URL || 'http://127.0.0.1:8787';
const ORIGINAL = {x:160, y:120, width:250, height:170, selected:false, documentRevision:0, sceneRevision:0};
const engines = [['chromium', chromium], ['firefox', firefox]];

async function ready() {
  for (let attempt=0; attempt<50; attempt++) {
    try { const response = await fetch(BASE + '/healthz'); if (response.ok) return; }
    catch { /* Server still starting. */ }
    await new Promise(resolve=>setTimeout(resolve,200));
  }
  throw new Error('Local SSR test server did not start: ' + BASE);
}

function modelPoint(rect,x,y) {
  assert.ok(rect,'Canvas must have a layout box');
  return { x:rect.x+(x/900)*rect.width, y:rect.y+(y/500)*rect.height };
}

async function getState(page) {
  return await page.evaluate(() => window.__uiPlatformDebug.snapshot());
}

async function clickCanvas(page,x,y) {
  const canvas=page.locator('#scene');
  await canvas.scrollIntoViewIfNeeded();
  const point=modelPoint(await canvas.boundingBox(),x,y);
  await page.mouse.click(point.x,point.y);
}

async function moveCanvas(page,fromX,fromY,toX,toY) {
  const canvas=page.locator('#scene');
  await canvas.scrollIntoViewIfNeeded();
  const rect=await canvas.boundingBox();
  const start=modelPoint(rect,fromX,fromY);
  const end=modelPoint(rect,toX,toY);
  await page.mouse.move(start.x,start.y);
  await page.mouse.down();
  await page.mouse.move(end.x,end.y,{steps:4});
  await page.mouse.up();
}

async function pixel(page,x,y) {
  return await page.evaluate(([px,py])=>Array.from(
    document.getElementById('scene').getContext('2d').getImageData(px,py,1,1).data
  ),[x,y]);
}

async function scenario(name, browser) {
  const page=await browser.newPage({viewport:{width:1440,height:1050},deviceScaleFactor:1});
  const errors=[];
  const requests=[];
  page.on('pageerror',err=>errors.push(err.message));
  page.on('request',req=>requests.push(req.url()));
  try {
    const html=await page.goto(BASE+'/',{waitUntil:'domcontentloaded'});
    assert.equal(html.status(),200);
    await page.waitForFunction(()=>window.__uiPlatformDebug?.mode==='Rust/WASM',null,{timeout:20000});
    assert.equal(await page.locator('#engine-status').textContent(),'Rust/WASM connected');
    assert.equal(await page.locator('meta[name="robots"]').getAttribute('content'),'noindex,nofollow');
    assert.equal(await page.locator('link[rel="canonical"]').getAttribute('href'),BASE+'/');
    assert.equal(JSON.parse(await page.locator('script[type="application/ld+json"]').textContent())['@type'],'WebApplication');
    assert.equal(await page.locator('h1').count(),1);
    assert.ok(requests.some(url=>url.endsWith('/assets/ui_wasm.wasm')),'Real WASM asset must be loaded');
    assert.ok(!requests.some(url=>url.includes('mock-engine')),'Never load the JS mock in product mode');
    assert.equal(await page.evaluate(()=>!!window.__uiPlatformMockEngine),false);
    assert.deepEqual(await getState(page),ORIGINAL);
    const wasmResponse=await page.request.get(BASE+'/assets/ui_wasm.wasm');
    assert.equal(wasmResponse.status(),200);
    assert.match(wasmResponse.headers()['content-type'],/^application\/wasm/i);

    await clickCanvas(page,210,180);
    let state=await getState(page);
    assert.equal(state.selected,true);
    assert.equal(state.documentRevision,0);
    assert.equal(state.sceneRevision,1);
    const paints=await page.evaluate(()=>window.__uiPlatformDebug.drawCount);
    await clickCanvas(page,210,180);
    assert.equal((await getState(page)).sceneRevision,1,'No-op must not advance scene revision');
    assert.equal(await page.evaluate(()=>window.__uiPlatformDebug.drawCount),paints,'No-op must not repaint');

    await page.locator('#rect-width').fill('320');
    await page.locator('#rect-width').press('Tab');
    state=await getState(page);
    assert.equal(state.width,320);
    assert.equal(state.documentRevision,1);
    assert.equal(await page.locator('#doc-revision').textContent(),'1');
    assert.equal((await pixel(page,443,197))[0],201,'DOM resize must redraw Canvas');
    await page.locator('#undo').click();
    state=await getState(page);
    assert.equal(state.width,250);
    assert.equal(state.documentRevision,2);
    assert.equal((await pixel(page,443,197))[0],248,'Undo must restore Canvas background');

    await moveCanvas(page,210,180,270,225);
    state=await getState(page);
    assert.equal(state.x,220,'Pointer movement must update Rust X');
    assert.equal(state.y,165,'Pointer movement must update Rust Y');
    assert.equal(await page.locator('#rect-x').inputValue(),'220','Canvas movement must update HTML');
    assert.match(await page.locator('#semantic-list').textContent(),/X 220, Y 165/);
    assert.equal((await pixel(page,170,130))[0],248,'Old object position must be cleared');
    assert.equal((await pixel(page,240,185))[0],201,'New object position must be painted');

    await page.locator('#scene').focus();
    await page.keyboard.press('ArrowRight');
    state=await getState(page);
    assert.equal(state.x,225,'Scene keyboard routing must advance Rust state');
    await page.locator('#reset').click();
    state=await getState(page);
    assert.deepEqual({x:state.x,y:state.y,width:state.width,height:state.height,selected:state.selected},
      {x:160,y:120,width:250,height:170,selected:false});

    await page.setViewportSize({width:390,height:844});
    await page.reload({waitUntil:'domcontentloaded'});
    await page.waitForFunction(()=>window.__uiPlatformDebug?.mode==='Rust/WASM',null,{timeout:20000});
    await clickCanvas(page,210,180);
    assert.equal((await getState(page)).selected,true,'Mobile viewport must preserve pointer mapping');
    assert.deepEqual(errors,[],'Unexpected browser JavaScript errors');

    await page.screenshot({path:'test-results/'+name+'-real-wasm.png',fullPage:true});
    console.log('PASS '+name+' real WASM browser, SSR/SEO, Canvas↔DOM, pointer/keyboard/undo/mobile');
  } catch (e) {
    await page.screenshot({path:'test-results/'+name+'-failure.png',fullPage:true}).catch(()=>{});
    console.error('FAIL '+name+': '+e.stack);
    console.error('Browser errors:',errors);
    throw e;
  } finally { await page.close(); }
}

await ready();
await mkdir('test-results',{recursive:true});
for (const [name,engine] of engines) {
  const browser=await engine.launch({headless:true});
  try {await scenario(name,browser);} finally {await browser.close();}
}
