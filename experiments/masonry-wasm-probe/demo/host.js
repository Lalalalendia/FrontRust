import initWasm from './gen/masonry_probe.js';
const canvas=document.querySelector('#masonry-canvas');
const ctx=canvas.getContext('2d',{alpha:false});
const el=id=>document.getElementById(id);
let engine=null;
let scheduled=false;
let lastSnapshot=null;
const required=[
  'mp_version','mp_init','mp_resize','mp_pointer','mp_rect_count','mp_rect_x',
  'mp_rect_y','mp_rect_width','mp_rect_height','mp_rect_rgba',
  'mp_layer_count','mp_access_nodes','mp_skipped_draws',
  'mp_click_count','mp_width','mp_height'
];

function sceneRects(){
  const count=engine.mp_rect_count();
  if(count<0||count>128)throw Error('Invalid WASM scene rect count: '+count);
  return Array.from({length:count},(_,i)=>({
    x:engine.mp_rect_x(i),y:engine.mp_rect_y(i),
    width:engine.mp_rect_width(i),height:engine.mp_rect_height(i),
    rgba:engine.mp_rect_rgba(i)>>>0
  }));
}
function snapshot(){
  return {engine:'Rust/Masonry/RenderRoot',
    width:engine.mp_width(),height:engine.mp_height(),
    layers:engine.mp_layer_count(),
    accessNodes:engine.mp_access_nodes(),
    skippedDraws:engine.mp_skipped_draws(),
    clicks:engine.mp_click_count(),rects:sceneRects()};
}
function paint(){
  scheduled=false;
  const scene=snapshot();
  ctx.fillStyle='#f8fbff';
  ctx.fillRect(0,0,canvas.width,canvas.height);
  for(const r of scene.rects){
    if(![r.x,r.y,r.width,r.height].every(Number.isFinite))throw Error('Nonfinite Masonry draw');
    const red=r.rgba&255,green=(r.rgba>>>8)&255,blue=(r.rgba>>>16)&255,alpha=(r.rgba>>>24)&255;
    ctx.fillStyle='rgba('+red+','+green+','+blue+','+(alpha/255)+')';
    ctx.fillRect(r.x,r.y,r.width,r.height);
  }
  el('layers').textContent=String(scene.layers);
  el('rectangles').textContent=String(scene.rects.length);
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
try{
  // Transitive wasm-bindgen imports require the generated JS glue; never instantiate raw bytes.
  engine=await initWasm();
  for(const name of required)if(typeof engine[name]!=='function')throw Error('Missing Masonry WASM ABI: '+name);
  if(engine.mp_version()!==1)throw Error('Unknown Masonry ABI revision');
  if(engine.mp_init(canvas.width,canvas.height)<2)throw Error('No real Masonry fills found');
  canvas.addEventListener('pointerdown',event=>{
    if(event.button!==0)return;
    const p=position(event);
    canvas.setPointerCapture(event.pointerId);
    engine.mp_pointer(1,p.x,p.y);
    requestPaint();
  });
  canvas.addEventListener('pointerup',event=>{
    if(event.button!==0)return;
    const p=position(event);
    engine.mp_pointer(0,p.x,p.y);
    requestPaint();
  });
  el('resize').addEventListener('click',()=>resize(canvas.width===320?400:320,canvas.height===240?300:240));
  requestPaint();
  el('status').textContent='Real Rust/WASM RenderRoot';
  window.__masonryProbe={snapshot,resize,get lastSnapshot(){return lastSnapshot}};
}catch(err){
  el('status').textContent='ERROR: '+err.message;
  console.error(err);
}