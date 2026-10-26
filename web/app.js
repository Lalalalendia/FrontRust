const $=id=>document.getElementById(id);
const canvas=$('scene'),ctx=canvas.getContext('2d',{alpha:false});
const fields=['rect-x','rect-y','rect-width','rect-height'];
const names=['uip_version','uip_select_at','uip_move_to','uip_set_width','uip_set_height','uip_undo','uip_reset','uip_get_x','uip_get_y','uip_get_width','uip_get_height','uip_is_selected','uip_document_revision','uip_scene_revision'];
let e,drag=null,lastRevision=-1,drawCount=0;
const snapshot=()=>({x:e.uip_get_x(),y:e.uip_get_y(),width:e.uip_get_width(),height:e.uip_get_height(),selected:!!e.uip_is_selected(),documentRevision:e.uip_document_revision(),sceneRevision:e.uip_scene_revision()});
async function loadEngine(){
  const res=await fetch('/assets/ui_wasm.wasm',{cache:'no-store'});
  if(!res.ok)throw Error('Real Rust/WASM not built: see README');
  const {instance}=await WebAssembly.instantiateStreaming(res,{});
  for(const n of names)if(typeof instance.exports[n]!=='function')throw Error('Missing WASM ABI: '+n);
  if(instance.exports.uip_version()!==1)throw Error('Unsupported WASM ABI version');
  return instance.exports;
}
function draw(s){
 if(lastRevision===s.sceneRevision)return;
 lastRevision=s.sceneRevision;drawCount++;
 ctx.fillStyle='#f8faff';ctx.fillRect(0,0,900,500);
 ctx.strokeStyle='#e0e6f3';ctx.lineWidth=1;
 for(let x=0;x<900;x+=30){ctx.beginPath();ctx.moveTo(x,0);ctx.lineTo(x,500);ctx.stroke();}
 for(let y=0;y<500;y+=30){ctx.beginPath();ctx.moveTo(0,y);ctx.lineTo(900,y);ctx.stroke();}
 ctx.fillStyle='#c9d6fb';ctx.fillRect(s.x,s.y,s.width,s.height);
 ctx.strokeStyle=s.selected?'#526bd4':'#8199d5';ctx.lineWidth=s.selected?3:1.5;
 ctx.strokeRect(s.x,s.y,s.width,s.height);
 ctx.fillStyle='#233f80';ctx.font='17px sans-serif';ctx.fillText('Object #1',s.x+15,s.y+30);
 if(s.selected){
  ctx.fillStyle='white';
  for(const [x,y] of [[s.x,s.y],[s.x+s.width,s.y],[s.x,s.y+s.height],[s.x+s.width,s.y+s.height]]){
   ctx.fillRect(x-5,y-5,10,10);ctx.strokeRect(x-5,y-5,10,10);
  }
 }
}
function refresh(){
 const s=snapshot();const values=[s.x,s.y,s.width,s.height];
 for(let i=0;i<fields.length;i++){const control=$(fields[i]);if(document.activeElement!==control)control.value=values[i];}
 $('selection-label').textContent=s.selected?'Выделен · объект #1':'Не выделен';
 $('doc-revision').textContent=s.documentRevision;
 $('scene-revision').textContent='Сцена '+s.sceneRevision;
 const li=document.createElement('li');
 li.textContent='Прямоугольник #1, X '+s.x+', Y '+s.y+', ширина '+s.width+', высота '+s.height;
 $('semantic-list').replaceChildren(li);
 draw(s);
}
function point(ev){const r=canvas.getBoundingClientRect();return {x:Math.round((ev.clientX-r.left)*900/r.width),y:Math.round((ev.clientY-r.top)*500/r.height)};}
function bind(){
 canvas.addEventListener('pointerdown',ev=>{
  if(ev.button!==0)return;
  canvas.focus();
  const p=point(ev);e.uip_select_at(p.x,p.y);
  const s=snapshot();drag=s.selected?{dx:p.x-s.x,dy:p.y-s.y}:null;
  if(drag)canvas.setPointerCapture(ev.pointerId);
  refresh();
 });
 canvas.addEventListener('pointermove',ev=>{if(!drag)return;const p=point(ev);e.uip_move_to(p.x-drag.dx,p.y-drag.dy);refresh();});
 for(const type of ['pointerup','pointercancel','lostpointercapture'])canvas.addEventListener(type,()=>drag=null);
 canvas.addEventListener('keydown',ev=>{
  if(!snapshot().selected)return;
  const moves={ArrowLeft:[-5,0],ArrowRight:[5,0],ArrowUp:[0,-5],ArrowDown:[0,5]};
  const delta=moves[ev.key];if(!delta)return;ev.preventDefault();const s=snapshot();e.uip_move_to(s.x+delta[0],s.y+delta[1]);refresh();
 });
 const apply=[n=>e.uip_move_to(n,snapshot().y),n=>e.uip_move_to(snapshot().x,n),n=>e.uip_set_width(n),n=>e.uip_set_height(n)];
 fields.forEach((id,i)=>$(id).addEventListener('change',()=>{
  const n=Number($(id).value);
  if(!Number.isFinite(n))return;
  if(!snapshot().selected){const s=snapshot();e.uip_select_at(s.x+1,s.y+1);}
  apply[i](Math.trunc(n));$(id).blur();refresh();
 }));
 $('undo').addEventListener('click',()=>{e.uip_undo();refresh();});
 $('reset').addEventListener('click',()=>{e.uip_reset();refresh();});
}
try{
 e=await loadEngine();
 bind();refresh();
 $('engine-status').textContent='Rust/WASM connected';
 window.__uiPlatformDebug={snapshot,get drawCount(){return drawCount;},mode:'Rust/WASM'};
}catch(err){
 $('engine-status').textContent=String(err.message);
 fields.concat(['undo','reset']).forEach(id=>$(id).disabled=true);
 console.error(err);
}
