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
    assert.ok(first.rects.length>=3);
    assert.equal(first.skippedDraws,0,'unsupported operations must fail the fidelity gate');
    assert.ok(first.ops.length>first.rects.length);
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
    // Upstream paint_border generates a real Draw::Stroke. Verify its color
    // at the middle of the top edge, not merely presence of a Rust ABI value.
    const strokeOp=first.ops.find(op=>op.kind===2&&op.strokeWidth>=3);
    assert.ok(strokeOp,'real Masonry stroke not serialized');
    const stroke=first.rects[strokeOp.shape];
    assert.ok(stroke,'stroke record missing');
    const borderPixel=await page.evaluate(({x,y})=>Array.from(
      document.querySelector('#masonry-canvas').getContext('2d')
        .getImageData(Math.floor(x),Math.floor(y),1,1).data
    ),{x:stroke.x+stroke.width/2,y:stroke.y});
    assert.ok(borderPixel[0]>190&&borderPixel[1]<120&&borderPixel[2]<160,
      'Real stroke did not reach Canvas pixels: '+JSON.stringify(borderPixel));
    // The Portal clips a 120px green child to an 80px viewport.
    const clipStart=first.ops.findIndex(op=>op.kind===3);
    const clipEnd=first.ops.findIndex(op=>op.kind===4);
    assert.ok(clipStart>=0&&clipEnd>clipStart,'clip stack absent');
    const green=first.rects.find(r=>(r.rgba&255)===40&&((r.rgba>>>8)&255)===170);
    assert.ok(green,'clipped green widget missing');
    const clip=first.rects[first.ops[clipStart].shape];
    assert.ok(clip.width<green.width,'Portal viewport must crop the green widget');
    const pixels=await page.evaluate(({inside,outside})=>{
      const ctx=document.querySelector('#masonry-canvas').getContext('2d');
      const get=({x,y})=>Array.from(ctx.getImageData(Math.floor(x),Math.floor(y),1,1).data);
      return {inside:get(inside),outside:get(outside)};
    },{
      inside:{x:clip.x+clip.width/2,y:green.y+green.height/2},
      outside:{x:clip.x+clip.width+12,y:green.y+green.height/2}
    });
    assert.ok(pixels.inside[1]>145&&pixels.inside[0]<90,
      'Green fill missing inside clip: '+JSON.stringify(pixels.inside));
    assert.ok(!(pixels.outside[1]>145&&pixels.outside[0]<90),
      'Clipped region leaked outside viewport: '+JSON.stringify(pixels.outside));
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
    console.log('PASS '+name+' real Masonry Draw::Stroke, Portal clip, solid rounded fills, ButtonPress and resize');
  }catch(error){
    await page.screenshot({path:'test-results/masonry-'+name+'-FAIL.png',fullPage:true}).catch(()=>{});
    console.error('FAIL '+name,error.stack,errors);throw error;
  }finally{await page.close();await browser.close();}
}
