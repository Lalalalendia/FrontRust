import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

const path=process.argv[2]||'experiments/masonry-wasm-probe/target/wasm32-unknown-unknown/release/frontrust_masonry_wasm_probe.wasm';
const moduleBytes=await readFile(path);
const {instance}=await WebAssembly.instantiate(moduleBytes,{});
const e=instance.exports;
const symbols=[
  'mp_version','mp_init','mp_resize','mp_pointer','mp_rect_count',
  'mp_rect_x','mp_rect_y','mp_rect_width','mp_rect_height','mp_rect_rgba',
  'mp_layer_count','mp_access_nodes','mp_skipped_draws','mp_click_count','mp_width','mp_height'
];
for(const s of symbols)assert.equal(typeof e[s],'function','missing exported '+s);
assert.equal(e.mp_version(),1);
assert.ok(e.mp_init(320,240)>=2,'real RenderRoot must emit filled shapes');
assert.ok(e.mp_layer_count()>0,'RenderRoot must emit VisualLayerPlan');
assert.ok(e.mp_access_nodes()>0,'real AccessKit tree must be nonempty');
const getRects=()=>Array.from({length:e.mp_rect_count()},(_,i)=>({
  x:e.mp_rect_x(i),y:e.mp_rect_y(i),
  width:e.mp_rect_width(i),height:e.mp_rect_height(i),rgba:e.mp_rect_rgba(i)>>>0
}));
const encode=(r,g,b)=>((r|(g<<8)|(b<<16)|(255<<24))>>>0);
const blue=encode(70,120,210),brown=encode(195,145,105);
let rects=getRects();
assert.ok(rects.some(r=>r.rgba===blue),'first painted surface from Masonry missing');
const target=rects.find(r=>r.rgba===brown);
assert.ok(target,'button child surface from Masonry missing');
assert.ok(target.width>0&&target.height>0,'button surface has no layout');
const x=target.x+target.width/2,y=target.y+target.height/2;
assert.equal(e.mp_click_count(),0);
e.mp_pointer(1,x,y);
e.mp_pointer(0,x,y);
assert.equal(e.mp_click_count(),1,'Masonry ButtonPress must be dispatched by RenderRoot');
assert.ok(e.mp_resize(400,300)>=2,'resize must repaint actual Masonry scene');
assert.deepEqual([e.mp_width(),e.mp_height()],[400,300]);
rects=getRects();
assert.ok(rects.some(r=>r.rgba===blue));
assert.ok(rects.some(r=>r.rgba===brown));
console.log('PASS real Masonry WASM RenderRoot layout, scene fill records, ButtonPress and resize');
