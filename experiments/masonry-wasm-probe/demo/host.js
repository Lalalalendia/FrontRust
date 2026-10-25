import initWasm from './gen/masonry_probe.js';
const canvas=document.querySelector('#masonry-canvas');
const ctx=canvas.getContext('2d',{alpha:false});
const el=id=>document.getElementById(id);
const semanticAction=el('masonry-action');
const inspector=el('masonry-inspector');
const actionAnchor=document.createComment('Rust AccessKit button projection');
semanticAction.before(actionAnchor);
let compositionActive=false;
let suppressNativeKeyboardClick=false;
let engine=null;
let scheduled=false;
let lastSnapshot=null;
const required=[
  'mp_version','mp_init','mp_resize','mp_pointer','mp_rect_count','mp_rect_x',
  'mp_rect_y','mp_rect_width','mp_rect_height','mp_rect_rgba',
  'mp_layer_count','mp_access_nodes','mp_skipped_draws',
  'mp_click_count','mp_width','mp_height',
  'mp_rect_local_x','mp_rect_local_y','mp_rect_local_width','mp_rect_local_height',
  'mp_rect_radius_tl','mp_rect_radius_tr','mp_rect_radius_br','mp_rect_radius_bl',
  'mp_rect_matrix_a','mp_rect_matrix_b','mp_rect_matrix_c',
  'mp_rect_matrix_d','mp_rect_matrix_e','mp_rect_matrix_f',
  'mp_op_count','mp_op_kind','mp_op_shape','mp_op_stroke_width',
  'mp_op_stroke_join','mp_op_miter_limit','mp_op_stroke_cap',
  'mp_rect_path_count','mp_rect_path_kind','mp_rect_path_arg',
  'mp_semantic_button_id','mp_focus_button','mp_focused_button',
  'mp_keyboard_button','mp_keyboard_event_count',
  'mp_access_button_id','mp_access_button_disabled','mp_access_focus_id',
  'mp_set_button_disabled','mp_set_button_present'
];


function focusOwner(){
  // Never treat visual DOM nodes as semantic owners. Only the projection
  // button or a real editor input can own keyboard interpretation.
  if(compositionActive)return 'composition';
  if(document.activeElement===semanticAction)return 'scene';
  if(document.activeElement===inspector)return 'inspector';
  return 'none';
}
function updateFocusLabels(){
  el('focus-owner').textContent='Фокус: '+focusOwner();
  el('composition-state').textContent='IME: '+(compositionActive?'композиция':'неактивна');
}
function handleSceneKey(event){
  if(event.key!=='Enter'&&event.key!==' ')return;
  // Suppress browser auto-click for the projected DOM button; the actual
  // ButtonPress must always come from Rust/Masonry TextEvent.
  event.preventDefault();
  if(compositionActive||event.isComposing||event.ctrlKey||event.metaKey||event.altKey)return;
  if(document.activeElement!==semanticAction)return;
  if(engine.mp_focused_button()!==1)throw Error('Scene key with no Rust focus');
  const code=event.key==='Enter'?1:2;
  if(engine.mp_keyboard_button(event.type==='keydown'?1:0,code)<0)
    throw Error('Rust refused focused Masonry key');
  if(event.type==='keyup'){
    // A browser may issue a default detail=0 click on key activation.
    // Rust already handled key-up: suppress that duplicate, not external
    // accessibility/assistive-tech click actions that have the same detail.
    suppressNativeKeyboardClick=true;
    queueMicrotask(()=>{suppressNativeKeyboardClick=false;});
  }
  requestPaint();
}
function sceneRects(){
  const count=engine.mp_rect_count();
  if(count<0||count>128)throw Error('Invalid WASM scene rect count: '+count);
  return Array.from({length:count},(_,i)=>({
    x:engine.mp_rect_x(i),y:engine.mp_rect_y(i),
    width:engine.mp_rect_width(i),height:engine.mp_rect_height(i),
    rgba:engine.mp_rect_rgba(i)>>>0,
    local:[engine.mp_rect_local_x(i),engine.mp_rect_local_y(i),
      engine.mp_rect_local_width(i),engine.mp_rect_local_height(i)],
    radii:[engine.mp_rect_radius_tl(i),engine.mp_rect_radius_tr(i),
      engine.mp_rect_radius_br(i),engine.mp_rect_radius_bl(i)],
    matrix:[engine.mp_rect_matrix_a(i),engine.mp_rect_matrix_b(i),engine.mp_rect_matrix_c(i),
      engine.mp_rect_matrix_d(i),engine.mp_rect_matrix_e(i),engine.mp_rect_matrix_f(i)],
    path:(()=>{
      const count=engine.mp_rect_path_count(i);
      if(count<0||count>512)throw Error('Invalid WASM path length: '+count);
      return Array.from({length:count},(_,j)=>({
        kind:engine.mp_rect_path_kind(i,j),
        args:Array.from({length:6},(_,k)=>engine.mp_rect_path_arg(i,j,k))
      }));
    })()
  }));
}
function sceneOps(){
  const count=engine.mp_op_count();
  if(count<0||count>256)throw Error('Invalid WASM scene operation count: '+count);
  return Array.from({length:count},(_,i)=>({
    kind:engine.mp_op_kind(i),
    shape:engine.mp_op_shape(i),
    strokeWidth:engine.mp_op_stroke_width(i),
    strokeJoin:engine.mp_op_stroke_join(i),
    miterLimit:engine.mp_op_miter_limit(i),
    strokeCap:engine.mp_op_stroke_cap(i)
  }));
}
function snapshot(){
  return {engine:'Rust/Masonry/RenderRoot',
    width:engine.mp_width(),height:engine.mp_height(),
    layers:engine.mp_layer_count(),
    accessNodes:engine.mp_access_nodes(),
    skippedDraws:engine.mp_skipped_draws(),
    clicks:engine.mp_click_count(),rects:sceneRects(),ops:sceneOps(),
    semanticButtonId:engine.mp_semantic_button_id(),
    accessButtonId:String(engine.mp_access_button_id()),
    accessButtonDisabled:engine.mp_access_button_disabled(),
    accessFocusId:String(engine.mp_access_focus_id()),
    focusedButton:engine.mp_focused_button(),
    keyboardEvents:engine.mp_keyboard_event_count(),
    composing:compositionActive,owner:focusOwner()};
}
function validateScene(scene){
  // Refuse incomplete snapshots BEFORE changing the displayed framebuffer.
  if(scene.skippedDraws!==0)throw Error('Partial Masonry scene: '+scene.skippedDraws+' unsupported operations');
  if(scene.rects.length>128||scene.ops.length>256)throw Error('Oversized Masonry scene');
  for(const r of scene.rects){
    if(r.path.length>512)throw Error('Oversized Masonry path');
    if(![...r.local,...r.radii,...r.matrix].every(Number.isFinite))
      throw Error('Nonfinite Masonry geometry');
    for(const command of r.path){
      if(![1,2,3,4,5].includes(command.kind)||!command.args.every(Number.isFinite))
        throw Error('Unsupported or nonfinite Masonry PathEl');
    }
  }
}
function syncAccessProjection(scene){
  // Only a real tagged AccessKit Button/Click node may own this DOM control.
  // A newly created Rust widget gets a new u64 NodeId, exported losslessly.
  const present=scene.accessButtonId!=='0'&&scene.semanticButtonId===1;
  if(!present){
    if(document.activeElement===semanticAction)inspector.focus({preventScroll:true});
    semanticAction.remove();
    semanticAction.dataset.accesskitId='';
  }else{
    if(!semanticAction.isConnected)actionAnchor.after(semanticAction);
    semanticAction.dataset.accesskitId=scene.accessButtonId;
    const hadSceneFocus=document.activeElement===semanticAction;
    semanticAction.disabled=scene.accessButtonDisabled!==0;
    // Chromium may automatically blur an element as soon as disabled=true.
    // Capture focus beforehand or it would be lost to the document body.
    if(hadSceneFocus&&(semanticAction.disabled||scene.focusedButton!==1))
      inspector.focus({preventScroll:true});
  }
  el('toggle-disabled').textContent=scene.accessButtonDisabled===1
    ?'Включить кнопку Rust':'Отключить кнопку Rust';
  el('toggle-disabled').disabled=!present;
  el('toggle-present').textContent=present
    ?'Удалить кнопку Rust':'Вернуть кнопку Rust';
  updateFocusLabels();
}
function paint(){
  scheduled=false;
  const scene=snapshot();
  validateScene(scene);
  syncAccessProjection(scene);
  ctx.fillStyle='#f8fbff';
  ctx.fillRect(0,0,canvas.width,canvas.height);
  // The scene is an ORDERED command stream. A clip is an actual Canvas
  // save/clip/restore scope, not a decorative bounding rectangle.
  const shapePath=r=>{
    ctx.beginPath();
    if(r.path.length){
      for(const step of r.path){
        if(!step.args.every(Number.isFinite))throw Error('Nonfinite Masonry PathEl');
        const p=step.args;
        switch(step.kind){
          case 1:ctx.moveTo(p[0],p[1]);break;
          case 2:ctx.lineTo(p[0],p[1]);break;
          case 3:ctx.quadraticCurveTo(p[0],p[1],p[2],p[3]);break;
          case 4:ctx.bezierCurveTo(p[0],p[1],p[2],p[3],p[4],p[5]);break;
          case 5:ctx.closePath();break;
          default:throw Error('Unsupported Masonry PathEl kind: '+step.kind);
        }
      }
    }else if(r.radii.some(v=>v>0))ctx.roundRect(...r.local,r.radii);
    else ctx.rect(...r.local);
  };
  let clipDepth=0;
  for(const op of scene.ops){
    if(op.kind===4){
      if(clipDepth===0)throw Error('Unbalanced Masonry clip pop');
      ctx.restore();clipDepth--;continue;
    }
    const r=scene.rects[op.shape];
    if(!r)throw Error('Invalid Masonry operation geometry index');
    if(![...r.local,...r.radii,...r.matrix].every(Number.isFinite))
      throw Error('Nonfinite Masonry local geometry');
    if(r.path.length>512)throw Error('Oversized Masonry path');
    if(op.kind===3){
      // Keep the clip in the global scene coordinates of the Rust transform.
      ctx.save();
      ctx.setTransform(...r.matrix);
      shapePath(r);
      ctx.clip('nonzero');
      ctx.setTransform(1,0,0,1,0,0);
      clipDepth++;
      continue;
    }
    if(op.kind!==1&&op.kind!==2)throw Error('Unsupported Masonry paint operation: '+op.kind);
    ctx.save();
    ctx.setTransform(...r.matrix);
    const red=r.rgba&255,green=(r.rgba>>>8)&255,blue=(r.rgba>>>16)&255,alpha=(r.rgba>>>24)&255;
    const color='rgba('+red+','+green+','+blue+','+(alpha/255)+')';
    shapePath(r);
    if(op.kind===2){
      if(!Number.isFinite(op.strokeWidth)||op.strokeWidth<=0)throw Error('Invalid stroke width');
      const joins=['miter','round','bevel'];
      if(!Number.isInteger(op.strokeJoin)||!joins[op.strokeJoin])
        throw Error('Unsupported Masonry stroke join: '+op.strokeJoin);
      if(!Number.isFinite(op.miterLimit)||op.miterLimit<=0)
        throw Error('Invalid Masonry stroke miter limit');
      ctx.strokeStyle=color;ctx.lineWidth=op.strokeWidth;
      const caps=['butt','round','square'];
      if(!Number.isInteger(op.strokeCap)||!caps[op.strokeCap])
        throw Error('Unsupported Masonry line cap: '+op.strokeCap);
      ctx.lineJoin=joins[op.strokeJoin];ctx.miterLimit=op.miterLimit;
      ctx.lineCap=caps[op.strokeCap];
      ctx.stroke();
    }else{
      ctx.fillStyle=color;ctx.fill('nonzero');
    }
    ctx.restore();
  }
  if(clipDepth!==0)throw Error('Unbalanced Masonry clip stack');
  el('layers').textContent=String(scene.layers);
  el('rectangles').textContent=String(scene.rects.length)+' / '+scene.ops.length+' ops';
  el('access').textContent=String(scene.accessNodes);
  el('clicks').textContent=String(scene.clicks);
  lastSnapshot=scene;
}
function requestPaint(){if(scheduled)return;scheduled=true;requestAnimationFrame(paint);}
function position(event){
  const rect=canvas.getBoundingClientRect();
  return {x:(event.clientX-rect.left)*(canvas.width/rect.width),
    y:(event.clientY-rect.top)*(canvas.height/rect.height)};
}
function resize(width,height){
  const count=engine.mp_resize(width,height);
  if(count<=0)throw Error('Masonry resize failed: '+count);
  canvas.width=engine.mp_width();canvas.height=engine.mp_height();
  requestPaint();
}
function setButtonDisabled(disabled){
  const result=engine.mp_set_button_disabled(disabled?1:0);
  if(result!==Number(disabled))throw Error('Rust AccessKit disabled update refused');
  requestPaint();
  return result;
}
function setButtonPresent(present){
  const result=engine.mp_set_button_present(present?1:0);
  if(result!==Number(present))throw Error('Rust AccessKit node lifecycle refused');
  requestPaint();
  return result;
}
try{
  // Transitive wasm-bindgen imports require the generated JS glue; never instantiate raw bytes.
  engine=await initWasm();
  for(const name of required)if(typeof engine[name]!=='function')throw Error('Missing Masonry WASM ABI: '+name);
  if(engine.mp_version()!==1)throw Error('Unknown Masonry ABI revision');
  if(engine.mp_init(canvas.width,canvas.height)<2)throw Error('No real Masonry fills found');
  if(engine.mp_semantic_button_id()!==1)
    throw Error('Tagged button missing matching AccessKit Button/Click node');
  semanticAction.dataset.semanticId=String(engine.mp_semantic_button_id());
  semanticAction.addEventListener('focus',()=>{
    if(engine.mp_focus_button(1)!==1)throw Error('Rust Button focus refused');
    updateFocusLabels();requestPaint();
  });
  semanticAction.addEventListener('blur',()=>{
    engine.mp_focus_button(0);updateFocusLabels();requestPaint();
  });
  semanticAction.addEventListener('keydown',handleSceneKey);
  semanticAction.addEventListener('keyup',handleSceneKey);
  semanticAction.addEventListener('click',event=>{
    if(compositionActive)return;
    // detail=0 also occurs for assistive-technology activation. Ignoring it
    // would leave a keyboard-addressable but non-operable DOM control.
    if(event.detail===0&&suppressNativeKeyboardClick)return;
    if(document.activeElement!==semanticAction)
      semanticAction.focus({preventScroll:true});
    if(engine.mp_focused_button()!==1)throw Error('DOM/Rust click focus mismatch');
    if(engine.mp_keyboard_button(1,1)<0||engine.mp_keyboard_button(0,1)<0)
      throw Error('Rust refused projected DOM activation');
    requestPaint();
  });
  inspector.addEventListener('focus',updateFocusLabels);
  inspector.addEventListener('blur',updateFocusLabels);
  inspector.addEventListener('compositionstart',()=>{
    compositionActive=true;updateFocusLabels();
  });
  inspector.addEventListener('compositionend',()=>{
    compositionActive=false;updateFocusLabels();
  });
  canvas.addEventListener('pointerdown',event=>{
    if(event.button!==0)return;
    const p=position(event);
    canvas.setPointerCapture(event.pointerId);
    engine.mp_pointer(1,p.x,p.y);
    // Pointer focus established by real Masonry Button; mirror it to the
    // one keyboard-addressable DOM semantic target.
    if(!compositionActive&&engine.mp_focused_button()===1)
      semanticAction.focus({preventScroll:true});
    requestPaint();
  });
  canvas.addEventListener('pointerup',event=>{
    if(event.button!==0)return;
    const p=position(event);
    engine.mp_pointer(0,p.x,p.y);
    requestPaint();
  });
  el('toggle-disabled').addEventListener('click',()=>{
    setButtonDisabled(engine.mp_access_button_disabled()!==1);
  });
  el('toggle-present').addEventListener('click',()=>{
    setButtonPresent(String(engine.mp_access_button_id())==='0');
  });
  el('resize').addEventListener('click',()=>resize(canvas.width===320?400:320,canvas.height===360?300:360));
  requestPaint();
  el('status').textContent='Real Rust/WASM RenderRoot';
  // Exposed only for deterministic browser harness assertions.
  updateFocusLabels();
  window.__masonryProbe={
    snapshot,resize,validateScene,focusOwner,setButtonDisabled,setButtonPresent,
    get composing(){return compositionActive},
    get lastSnapshot(){return lastSnapshot}
  };
}catch(err){
  el('status').textContent='ERROR: '+err.message;
  console.error(err);
}