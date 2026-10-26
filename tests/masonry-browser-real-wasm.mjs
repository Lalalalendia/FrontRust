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

    // R4a: DOM focus is projected onto the real tagged Rust/Masonry Button.
    // Keep R3b pointer click count (=1) and prove two independent keyboard
    // activation paths from actual browser key events.
    const semantic=page.locator('#masonry-action');
    const inspector=page.locator('#masonry-inspector');
    assert.equal(await semantic.getAttribute('data-semantic-id'),'1');
    await semantic.focus();
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().focusedButton===1);
    const startEvents=await page.evaluate(()=>window.__masonryProbe.snapshot().keyboardEvents);
    await page.keyboard.press('Enter');
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().clicks===2);
    await page.keyboard.press('Space');
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().clicks===3);
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().keyboardEvents),startEvents+4);
    // Browser-native Tab order moves into the inspector, and the Rust scene
    // loses focus. Delete/Enter/Undo while typing MUST NOT reach the scene.
    await page.keyboard.press('Tab');
    assert.equal(await inspector.evaluate(el=>document.activeElement===el),true);
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().focusedButton===0);
    const fenced=await page.evaluate(()=>window.__masonryProbe.snapshot());
    await inspector.fill('editor text');
    await page.keyboard.press('Delete');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Control+z');
    const afterInspector=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(afterInspector.keyboardEvents,fenced.keyboardEvents);
    assert.equal(afterInspector.clicks,3);
    assert.equal(afterInspector.owner,'inspector');
    // Native editor Undo may change its text; what matters is that subsequent
    // scene focus/composition must not edit the inspector behind the user's back.
    const inspectorValueAfterLocalUndo=await inspector.inputValue();
    // Simulated composition verifies the explicit browser composition fence;
    // this is NOT proof of OS-level RU/JP IME correctness.
    await inspector.evaluate(el=>el.dispatchEvent(new CompositionEvent('compositionstart',{
      bubbles:true,data:'に'
    })));
    assert.equal(await page.evaluate(()=>window.__masonryProbe.composing),true);
    await semantic.focus();
    assert.equal(await page.evaluate(()=>window.__masonryProbe.focusOwner()),'composition');
    const beforeComposition=await page.evaluate(()=>window.__masonryProbe.snapshot());
    await page.keyboard.press('Enter');
    await page.keyboard.press('Space');
    const blocked=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(blocked.clicks,beforeComposition.clicks,'IME must fence scene activation');
    assert.equal(blocked.keyboardEvents,beforeComposition.keyboardEvents,'IME leaked keys to Rust');
    await inspector.evaluate(el=>el.dispatchEvent(new CompositionEvent('compositionend',{
      bubbles:true,data:'に'
    })));
    assert.equal(await page.evaluate(()=>window.__masonryProbe.composing),false);
    await page.keyboard.press('Enter');
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().clicks===4);
    await inspector.focus();
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().focusedButton),0);
    assert.equal(await inspector.inputValue(),inspectorValueAfterLocalUndo,
      'Scene keyboard routing mutated inspector text after editor-local Undo');
    // R4b: AccessKit role/action-gated DOM button must also accept detail=0
    // (assistive-technology-style) click; Enter's own native default must
    // not produce a duplicate Rust ButtonPress.
    const beforeA11y=await page.evaluate(()=>window.__masonryProbe.snapshot().clicks);
    await semantic.evaluate(el=>el.click());
    await page.waitForFunction(expected=>
      window.__masonryProbe.snapshot().clicks===expected,beforeA11y+1);
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().focusedButton),1);
    await semantic.click();
    await page.waitForFunction(expected=>
      window.__masonryProbe.snapshot().clicks===expected,beforeA11y+2);
    await page.keyboard.press('Enter');
    await page.waitForFunction(expected=>
      window.__masonryProbe.snapshot().clicks===expected,beforeA11y+3);
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().clicks),beforeA11y+3,
      'Native keyboard click duplicated the Rust TextEvent activation');
    assert.equal(await inspector.inputValue(),inspectorValueAfterLocalUndo);
    await page.locator('#resize').click();
    await page.waitForFunction(()=>window.__masonryProbe.snapshot().width===400,{timeout:10000});
    const resized=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(resized.height,300);
    assert.ok(resized.rects.some(r=>(r.rgba&255)===70));
    // R4c: live AccessKit properties and actual Rust widget lifecycle must
    // project to DOM. Execute while the semantic button OWNS focus.
    await semantic.focus();
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().focusedButton===1);
    const originalId=await semantic.getAttribute('data-accesskit-id');
    assert.ok(originalId&&originalId!=='0');
    assert.equal(await page.evaluate(()=>
      window.__masonryProbe.snapshot().accessFocusId),
      originalId,'focused AccessKit NodeId must equal projected DOM identity');
    await page.evaluate(()=>window.__masonryProbe.setButtonDisabled(true));
    await page.waitForFunction(()=>document.querySelector('#masonry-action')?.disabled===true);
    assert.equal(await semantic.getAttribute('data-accesskit-id'),originalId);
    assert.equal(await inspector.evaluate(el=>document.activeElement===el),true,
      'disabling focused Rust button must transfer focus to inspector');
    const disabled=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(disabled.accessButtonDisabled,1);
    assert.equal(disabled.focusedButton,0);
    const disabledClicks=disabled.clicks;
    await page.evaluate(()=>window.__masonryProbe.setButtonDisabled(false));
    await page.waitForFunction(()=>document.querySelector('#masonry-action')?.disabled===false);
    assert.equal(await semantic.getAttribute('data-accesskit-id'),originalId);
    await page.evaluate(()=>window.__masonryProbe.setButtonPresent(false));
    await page.waitForFunction(()=>document.querySelector('#masonry-action')===null);
    const missing=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(missing.accessButtonId,'0');
    assert.equal(missing.semanticButtonId,-1);
    assert.equal(missing.clicks,disabledClicks);
    await page.evaluate(()=>window.__masonryProbe.setButtonPresent(true));
    await page.waitForFunction(()=>document.querySelector('#masonry-action')?.dataset.accesskitId);
    const replacementId=await semantic.getAttribute('data-accesskit-id');
    assert.ok(replacementId&&replacementId!=='0'&&replacementId!==originalId,
      'recreated Rust widget must project a NEW AccessKit NodeId');
    await semantic.focus();
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().focusedButton===1);
    await page.keyboard.press('Enter');
    await page.waitForFunction(expected=>
      window.__masonryProbe.snapshot().clicks===expected,disabledClicks+1);
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().clicks),
      disabledClicks+1,'recreated button doubled Enter activation');
    // R4d: use native browser input events for ordinary Unicode typing and
    // explicitly bounded composition lifecycle events for genuine Rust TextArea.
    const editor=page.locator('#masonry-text-editor');
    const sceneBeforeEditor=await page.evaluate(()=>window.__masonryProbe.snapshot());
    await editor.fill('Привет ');
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет ');
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().editorFocused),1);
    await page.keyboard.type('🌍');
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет 🌍');
    const beforePreedit=await page.evaluate(()=>window.__masonryProbe.snapshot());
    await editor.evaluate(el=>{
      el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
      el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'に'}));
    });
    const duringPreedit=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(duringPreedit.editorCommitted,'Привет 🌍',
      'IME preedit was persisted');
    assert.notEqual(duringPreedit.editorLive,duringPreedit.editorCommitted);
    assert.equal(duringPreedit.editorComposing,1);
    assert.equal(duringPreedit.owner,'composition');
    await editor.evaluate(el=>el.dispatchEvent(
      new CompositionEvent('compositionupdate',{bubbles:true,data:'日本'})));
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().editorCommitted),
      'Привет 🌍');
    await editor.evaluate(el=>el.dispatchEvent(
      new CompositionEvent('compositionend',{bubbles:true,data:'日本'})));
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет 🌍日本');
    const afterCommit=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(afterCommit.editorCommits,beforePreedit.editorCommits+1);
    assert.equal(afterCommit.editorComposing,0);
    await page.waitForFunction(()=>document.querySelector('#masonry-text-editor')?.value==='Привет 🌍日本');
    await editor.evaluate(el=>{
      el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
      el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'未確定'}));
      el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:''}));
    });
    const canceled=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(canceled.editorCommitted,'Привет 🌍日本','canceled composition mutated committed text');
    assert.equal(canceled.editorLive,'Привет 🌍日本');
    assert.equal(canceled.editorCommits,afterCommit.editorCommits);
    await page.keyboard.press('Enter');
    const fencedEditor=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(fencedEditor.clicks,sceneBeforeEditor.clicks,
      'editor keystrokes must not activate Masonry scene');
    assert.equal(fencedEditor.keyboardEvents,sceneBeforeEditor.keyboardEvents);
    // R4e: browser selection indices are UTF-16; Rust accepts UTF-8 byte
    // boundaries only. Exercise a caret in the middle and selected emoji.
    await editor.fill('Привет 🌍日本');
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет 🌍日本');
    await editor.evaluate(el=>el.setSelectionRange(7,7));
    await page.keyboard.type('X');
    const insertedMid=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(insertedMid.editorCommitted,'Привет X🌍日本',
      'native browser input did not commit mid-string insertion; '+JSON.stringify({
        editorCommitted:insertedMid.editorCommitted,
        domValue:await editor.inputValue(),errors}));
    assert.deepEqual(await editor.evaluate(el=>[el.selectionStart,el.selectionEnd]),[8,8],
      'ordinary mid-string insertion moved browser caret to end');
    await editor.evaluate(el=>el.setSelectionRange(8,10)); // full emoji surrogate pair
    await page.keyboard.press('Backspace');
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет X日本');
    assert.deepEqual(await editor.evaluate(el=>[el.selectionStart,el.selectionEnd]),[8,8],
      'selected emoji deletion did not preserve collapsed caret');
    await editor.evaluate(el=>el.setSelectionRange(7,8));
    assert.deepEqual(await editor.evaluate(el=>[el.selectionStart,el.selectionEnd]),[7,8],
      'Firefox/Chromium must preserve native selected UTF-16 range');
    await page.waitForFunction(()=>{
      const selection=window.__masonryProbe.snapshot().editorSelectionBytes;
      return selection?.start===13&&selection?.end===14;
    },null,{timeout:5000});
    // Playwright keyboard.type('猫') emulates key presses for a character
    // without a portable physical keyboard key. Firefox may insert it at
    // EOF instead of replacing the selection. insertText uses the browser's
    // native Unicode insertion path (input event, not a forged DOM event).
    // Actual OS IME remains a separate acceptance gate.
    await page.keyboard.insertText('猫');
    const cjkInsert=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(cjkInsert.editorCommitted,'Привет 猫日本',
      'CJK insertion must replace the middle selection: '+JSON.stringify({
        rust:cjkInsert.editorCommitted,dom:await editor.inputValue(),
        selection:await editor.evaluate(el=>[el.selectionStart,el.selectionEnd]),errors}));
    assert.deepEqual(await editor.evaluate(el=>[el.selectionStart,el.selectionEnd]),[8,8]);
    assert.equal(await editor.inputValue(),'Привет 猫日本');

    // R4e2: composition over a selected X is a single transaction against
    // the ORIGINAL range. Preedit is visible only in the Rust preview,
    // cancel/blur restore X and a late input cannot double-commit.
    await editor.fill('Привет X日本');
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет X日本');
    await editor.evaluate(el=>el.setSelectionRange(7,8));
    await page.waitForFunction(()=>{
      const selection=window.__masonryProbe.snapshot().editorSelectionBytes;
      return selection?.start===13&&selection?.end===14;
    },null,{timeout:5000});
    const beforeSelectedIme=await page.evaluate(()=>window.__masonryProbe.snapshot());
    await editor.evaluate(el=>{
      el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
      el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'猫'}));
    });
    const selectedPreedit=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(selectedPreedit.editorCommitted,'Привет X日本',
      'selected IME preedit prematurely committed text');
    assert.equal(selectedPreedit.editorLive,'Привет 猫日本',
      'selected preedit incorrectly appended instead of replacing X');
    assert.equal(selectedPreedit.editorComposing,1);
    await editor.evaluate(el=>el.dispatchEvent(
      new CompositionEvent('compositionend',{bubbles:true,data:'猫'})));
    await page.waitForFunction(()=>
      window.__masonryProbe.snapshot().editorCommitted==='Привет 猫日本');
    const selectedCommitted=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(selectedCommitted.editorCommits,beforeSelectedIme.editorCommits+1);
    await editor.evaluate(el=>{
      el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'猫'}));
      el.dispatchEvent(new InputEvent('input',
        {bubbles:true,data:'猫',inputType:'insertFromComposition'}));
    });
    const lateCommit=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(lateCommit.editorCommits,selectedCommitted.editorCommits,
      'late input or duplicate compositionend committed a second time');
    assert.equal(lateCommit.editorCommitted,'Привет 猫日本');
    await editor.fill('Привет X日本');
    await editor.evaluate(el=>el.setSelectionRange(7,8));
    await page.waitForFunction(()=>{
      const selection=window.__masonryProbe.snapshot().editorSelectionBytes;
      return selection?.start===13&&selection?.end===14;
    },null,{timeout:5000});
    await editor.evaluate(el=>{
      el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
      el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'未確定'}));
      el.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:''}));
    });
    const selectedCanceled=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(selectedCanceled.editorCommitted,'Привет X日本');
    assert.equal(selectedCanceled.editorLive,'Привет X日本');
    assert.equal(selectedCanceled.editorCommits,selectedCommitted.editorCommits,
      'cancel incremented commit count');
    // A composition abandoned by blur must also leave the selected text.
    // An HTML input may already hold [7,8] after canceled composition.
    // Setting the same range does NOT guarantee another asynchronous 'select'
    // event. Assert the actual DOM range; compositionstart itself captures it
    // and synchronously calls real Rust mp_editor_begin_composition.
    await editor.evaluate(el=>el.setSelectionRange(7,8));
    assert.deepEqual(await editor.evaluate(el=>[el.selectionStart,el.selectionEnd]),[7,8]);
    assert.deepEqual(await page.evaluate(()=>
      window.__masonryProbe.snapshot().editorSelectionBytes),{start:13,end:14});
    await editor.evaluate(el=>{
      el.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));
      el.dispatchEvent(new CompositionEvent('compositionupdate',{bubbles:true,data:'仮'}));
    });
    await editor.blur();
    assert.equal(await page.evaluate(()=>window.__masonryProbe.snapshot().editorFocused),0);
    const afterBlur=await page.evaluate(()=>window.__masonryProbe.snapshot());
    assert.equal(afterBlur.editorCommitted,'Привет X日本');
    assert.equal(afterBlur.editorLive,'Привет X日本');
    const afterSplice=afterBlur;
    assert.equal(afterSplice.clicks,sceneBeforeEditor.clicks);
    assert.equal(afterSplice.keyboardEvents,sceneBeforeEditor.keyboardEvents);
    assert.deepEqual(errors,[]);
    await page.screenshot({path:'test-results/masonry-'+name+'.png',fullPage:true});
    console.log('PASS '+name+' R4e2 selected IME commit/cancel/blur + R4e Unicode splice + R4d/R4c/R3b');
  }catch(error){
    await page.screenshot({path:'test-results/masonry-'+name+'-FAIL.png',fullPage:true}).catch(()=>{});
    console.error('FAIL '+name,error.stack,errors);throw error;
  }finally{await page.close();await browser.close();}
}
