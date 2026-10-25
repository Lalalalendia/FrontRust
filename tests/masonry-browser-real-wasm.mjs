import assert from 'node:assert/strict';
import {mkdir} from 'node:fs/promises';
import {chromium,firefox} from 'playwright';

const URL=process.env.FRONT_RUST_MASONRY_URL||'http://127.0.0.1:8792/';
for(let i=0;i<50;i++){
  try{const r=await fetch(URL);if(r.ok)break;}catch{}
  if(i===49)throw Error('Masonry browser demo server not ready');
  await new Promise(resolve=>setTimeout(resolve,200));
}
await mkdir('test-results',{recursive:true});
const expected=[70,120,210,255];
const matchPixel=(got,want)=>got.every((value,i)=>Math.abs(value-want[i])<=1);
for(const [name,runner] of [['chromium',chromium],['firefox',firefox]]){
  const browser=await runner.launch({headless:true});
  const page=await browser.newPage({viewport:{width:1200,height:900}});
  const errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  try{
    await page.goto(URL,{waitUntil:'domcontentloaded'});
    await page.waitForFunction(()=>window.__masonryProbe?.lastSnapshot?.layers>0,{timeout:20000});
    assert.equal(await page.locator('#status').textContent(),'Real Rust/WASM RenderRoot');
    const first=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(first.engine,'Rust/Masonry/RenderRoot');
    assert.ok(first.layers>0);
    assert.ok(first.accessNodes>0);
    assert.ok(first.rects.length>=2);
    const blue=first.rects.find(r=>(r.rgba&255)===70&&((r.rgba>>>8)&255)===120);
    const brown=first.rects.find(r=>(r.rgba&255)===195&&((r.rgba>>>8)&255)===145);
    assert.ok(blue&&brown,'Both colored rectangles must come from actual Masonry paint scene');
    assert.ok(blue.radii[0]>=10&&blue.radii[2]>=10,'Real Masonry rounded corners must reach the browser');
    assert.ok(blue.matrix.every(Number.isFinite),'Real Kurbo affine matrix must reach the browser');
    const rgba=await page.evaluate(({x,y})=>Array.from(
      document.getElementById('masonry-canvas').getContext('2d')
        .getImageData(Math.floor(x),Math.floor(y),1,1).data
    ),{x:blue.x+blue.width/2,y:blue.y+blue.height/2});
    assert.ok(matchPixel(rgba,expected),'Canvas must render Masonry scene color, got '+JSON.stringify(rgba));
    // Pixel near the top-left corner is OUTSIDE a true rounded fill, but
    // WOULD be colored by the former rectangular bounding-box approximation.
    const corner=await page.evaluate(({x,y})=>Array.from(
      document.getElementById('masonry-canvas').getContext('2d')
        .getImageData(Math.floor(x),Math.floor(y),1,1).data
    ),{x:blue.x+1,y:blue.y+1});
    assert.ok(!matchPixel(corner,expected),'Rounded corner was flattened to a rectangular fill');
    const canvas=page.locator('#masonry-canvas');
    const box=await canvas.boundingBox();
    assert.ok(box);
    await page.mouse.click(box.x+(brown.x+brown.width/2)*box.width/first.width,
      box.y+(brown.y+brown.height/2)*box.height/first.height);
    // Rust action is synchronous; DOM is intentionally committed on requestAnimationFrame.
    // Wait for BOTH layers to update instead of racing Firefox's next animation frame.
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().clicks===1,{timeout:10000});
    await page.waitForFunction(()=>document.querySelector('#clicks')?.textContent==='1',{timeout:10000});
    assert.equal(await page.locator('#clicks').textContent(),'1');
    await page.locator('#resize').click();
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().width===400,{timeout:10000});
    const resized=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(resized.height,300);
    assert.ok(resized.rects.some(r=>(r.rgba&255)===70));
    assert.deepEqual(errors,[]);
    await page.screenshot({path:'test-results/masonry-'+name+'.png',fullPage:true});
    console.log('PASS '+name+' actual Masonry RenderRoot→WASM→Canvas2D fill, pointer ButtonPress, resize');
  }catch(error){
    await page.screenshot({path:'test-results/masonry-'+name+'-FAIL.png',fullPage:true}).catch(()=>{});
    console.error('FAIL '+name,error.stack,errors);throw error;
  }finally{await page.close();await browser.close();}
}
