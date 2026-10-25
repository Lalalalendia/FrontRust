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
    assert.equal(first.height,360,'full R3b Masonry scene must not be clipped by a 240px host');
    assert.ok(first.accessNodes>0);
    assert.ok(first.rects.length>=3);
    assert.equal(first.skippedDraws,0,'unsupported operations must fail the fidelity gate');
    assert.ok(first.ops.length>first.rects.length);
    let depth=0,maxDepth=0,greenDepth=0;
    const realGreen=first.rects.findIndex(r=>(r.rgba&255)===40&&((r.rgba>>>8)&255)===170);
    for(const op of first.ops){
      if(op.kind===3){depth++;maxDepth=Math.max(depth,maxDepth);}
      if(op.kind===1&&op.shape===realGreen)greenDepth=depth;
      if(op.kind===4){depth--;assert.ok(depth>=0);}
    }
    assert.equal(depth,0,'nested clip depth not balanced');
    assert.ok(maxDepth>=2&&greenDepth>=2,'two real Portal clipping scopes not retained');
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
    assert.equal(strokeOp.strokeJoin,0,'Masonry border miter join lost');
    assert.ok(first.ops.some(op=>op.kind===2&&op.strokeJoin===1&&op.strokeWidth<2),
      'Masonry scrollbar round stroke join was not delivered to browser');
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
    // Real Masonry DisclosureButton (open BezPath) is transformed by a
    // nontrivial Rust WidgetMut::set_transform, then replayed into Canvas2D.
    const magenta=first.rects.find(r=>(r.rgba&255)===230
      &&((r.rgba>>>8)&255)===50&&r.path.length>0);
    assert.ok(magenta,'Masonry BezPath did not cross the WASM bridge');
    assert.ok(Math.abs(magenta.matrix[1])>0.1&&Math.abs(magenta.matrix[2])>0.1,
      'Rust transform matrix was flattened');
    assert.ok(magenta.path.some(p=>p.kind===1)&&magenta.path.some(p=>p.kind===2));
    const initial=magenta.path.find(p=>p.kind===1).args;
    const following=magenta.path.find(p=>p.kind===2).args;
    const midX=(initial[0]+following[0])/2, midY=(initial[1]+following[1])/2;
    const [a,b,c,d,e,f]=magenta.matrix;
    const rendered={x:a*midX+c*midY+e,y:b*midX+d*midY+f};
    assert.ok(rendered.y>2&&rendered.y<first.height-2,
      'Real rotated DisclosureButton midpoint falls outside the host viewport');
    const samples=await page.evaluate(({x,y})=>{
      const ctx=document.querySelector('#masonry-canvas').getContext('2d');
      const out=[];
      for(let dy=-3;dy<=3;dy++)for(let dx=-3;dx<=3;dx++){
        const px=Math.floor(x+dx),py=Math.floor(y+dy);
        if(px>=0&&py>=0&&px<ctx.canvas.width&&py<ctx.canvas.height)
          out.push(Array.from(ctx.getImageData(px,py,1,1).data));
      }
      return out;
    },rendered);
    assert.ok(samples.some(c=>c[0]>150&&c[1]<120&&c[2]>90),
      'Rotated real Masonry BezPath stroke did not paint the expected Canvas pixels');

    // Real upstream Masonry Canvas recorded two distinct Bezier strokes in
    // Rust. Evaluate the independent Bernstein midpoint and sample actual
    // Chromium/Firefox Canvas pixels, both ON and OFF each curve.
    const getBezier=(rgb,kind)=>{
      const r=first.rects.find(r=>(r.rgba&255)===rgb[0]
        &&((r.rgba>>>8)&255)===rgb[1]
        &&((r.rgba>>>16)&255)===rgb[2]
        &&r.path.some(p=>p.kind===kind));
      assert.ok(r,'Real Masonry Canvas '+(kind===3?'QuadTo':'CurveTo')+' absent from scene');
      assert.equal(r.path.length,2);
      const start=r.path[0],curve=r.path[1];
      assert.equal(start.kind,1);
      assert.equal(curve.kind,kind);
      const p=start.args,k=curve.args;
      const local=kind===3
        ?{x:(p[0]+2*k[0]+k[2])/4,y:(p[1]+2*k[1]+k[3])/4}
        :{x:(p[0]+3*k[0]+3*k[2]+k[4])/8,y:(p[1]+3*k[1]+3*k[3]+k[5])/8};
      const [a,b,c,d,e,f]=r.matrix;
      return {r,local,position:{x:a*local.x+c*local.y+e,y:b*local.x+d*local.y+f}};
    };
    const quad=getBezier([10,215,235],3),cubic=getBezier([240,165,45],4);
    for(const curve of [quad,cubic]){
      assert.ok(curve.position.x>2&&curve.position.x<first.width-2
        &&curve.position.y>2&&curve.position.y<first.height-2,
        'Real Bezier midpoint lies outside the 320x360 host viewport');
    }
    assert.ok(Math.abs(quad.local.x-55)<0.001&&Math.abs(quad.local.y-30)<0.001);
    assert.ok(Math.abs(cubic.local.x-55.25)<0.001&&Math.abs(cubic.local.y-14.5)<0.001);
    const pixelProbes=await page.evaluate(({positions,offCurve})=>{
      const ctx=document.querySelector('#masonry-canvas').getContext('2d');
      const get=(x,y)=>Array.from(ctx.getImageData(Math.floor(x),Math.floor(y),1,1).data);
      const sample=({x,y})=>{
        const pixels=[];
        for(let dy=-2;dy<=2;dy++)for(let dx=-2;dx<=2;dx++)pixels.push(get(x+dx,y+dy));
        return pixels;
      };
      return {
        on:positions.map(sample),
        off:offCurve.map(({x,y})=>get(x,y))
      };
    },{
      positions:[quad.position,cubic.position],
      offCurve:[
        {x:quad.position.x,y:quad.position.y+18},
        {x:cubic.position.x,y:cubic.position.y+30}
      ]
    });
    const hits=(pixels,rgb)=>pixels.some(c=>c.slice(0,3).every((v,i)=>Math.abs(v-rgb[i])<=2));
    assert.ok(hits(pixelProbes.on[0],[10,215,235]),
      'QuadTo midpoint is not cyan in browser pixels: '+JSON.stringify(pixelProbes.on[0]));
    assert.ok(hits(pixelProbes.on[1],[240,165,45]),
      'CurveTo midpoint is not amber in browser pixels: '+JSON.stringify(pixelProbes.on[1]));
    assert.ok(!hits([pixelProbes.off[0]],[10,215,235]),'QuadTo painted away from expected curve');
    assert.ok(!hits([pixelProbes.off[1]],[240,165,45]),'CurveTo painted away from expected curve');

    // Negative browser gate: paint preflight must refuse an intentionally
    // partial/oversized scene instead of presenting incomplete graphics.
    const rejected=await page.evaluate(()=>{
      const original=window.__masonryProbe.snapshot();
      const results=[];
      for(const mutation of [
        {...original,skippedDraws:1},
        {...original,rects:original.rects.map((r,i)=>i===0
          ? {...r,path:Array.from({length:513},()=>({kind:2,args:[0,0,0,0,0,0]}))}
          :r)}
      ]){
        try{window.__masonryProbe.validateScene(mutation);results.push('ACCEPTED');}
        catch(e){results.push(String(e.message));}
      }
      return results;
    });
    assert.match(rejected[0],/Partial Masonry scene/);
    assert.match(rejected[1],/Oversized Masonry path/);
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
    console.log('PASS '+name+' real Masonry R3b QuadTo+CurveTo midpoint pixels, negative fail-closed, nested clips and ButtonPress');
  }catch(error){
    await page.screenshot({path:'test-results/masonry-'+name+'-FAIL.png',fullPage:true}).catch(()=>{});
    console.error('FAIL '+name,error.stack,errors);throw error;
  }finally{await page.close();await browser.close();}
}
