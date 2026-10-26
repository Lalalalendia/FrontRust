import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

const wasmGlue=await import('../experiments/masonry-wasm-probe/demo/gen/masonry_probe.js');
const {default:initWasm}=wasmGlue;
const path=process.argv[2]||'experiments/masonry-wasm-probe/demo/gen/masonry_probe_bg.wasm';
const bytes=await readFile(path);
// Pass bytes directly: no fetch is required by wasm-bindgen in Node.
// Legacy C ABI is scalar, while UTF-8 Rust String functions need the
// generated wasm-bindgen JS wrapper. Calling raw exports yields [ptr,len].
const e={...await initWasm(bytes),...wasmGlue};
assert.equal(typeof wasmGlue.mp_editor_committed_text,'function',
  'generated wasm-bindgen UTF-8 String wrapper is required');
assert.equal(e.mp_editor_committed_text,wasmGlue.mp_editor_committed_text);
const symbols=[
  'mp_version','mp_init','mp_resize','mp_pointer','mp_rect_count',
  'mp_rect_x','mp_rect_y','mp_rect_width','mp_rect_height','mp_rect_rgba',
  'mp_layer_count','mp_access_nodes','mp_skipped_draws',
  'mp_glyph_runs','mp_outlined_glyphs','mp_demo_font_id','mp_click_count','mp_width','mp_height',
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
  'mp_set_button_disabled','mp_set_button_present',
  'mp_editor_focus','mp_editor_focused','mp_editor_composing',
  'mp_editor_replace','mp_editor_preedit','mp_editor_update_preedit',
  'mp_editor_commit','mp_editor_cancel','mp_editor_committed_text',
  'mp_editor_live_text','mp_editor_commits','mp_editor_actions',
  'mp_editor_select_range','mp_editor_splice',
  'mp_editor_undo','mp_editor_redo',
  'mp_editor_undo_depth','mp_editor_redo_depth',
  'mp_demo_text','mp_demo_set_text'
];
for(const s of symbols)assert.equal(typeof e[s],'function','missing exported '+s);
assert.equal(e.mp_version(),1);
assert.ok(e.mp_init(320,360)>=2,'real RenderRoot must emit filled shapes');
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
const rounded=rects.findIndex(r=>r.rgba===blue);
assert.ok(rounded>=0);
assert.ok(e.mp_rect_radius_tl(rounded)>=10,'Masonry rounded corner must survive scene serialization');
assert.ok(e.mp_rect_radius_br(rounded)>=10,'Masonry bottom-right radius missing');
assert.ok(e.mp_rect_local_width(rounded)>0 && e.mp_rect_local_height(rounded)>0);
assert.ok(Number.isFinite(e.mp_rect_matrix_a(rounded)) && Number.isFinite(e.mp_rect_matrix_f(rounded)));
assert.ok([e.mp_rect_matrix_a(rounded),e.mp_rect_matrix_b(rounded),
  e.mp_rect_matrix_c(rounded),e.mp_rect_matrix_d(rounded),
  e.mp_rect_matrix_e(rounded),e.mp_rect_matrix_f(rounded)].every(Number.isFinite));
const ops=Array.from({length:e.mp_op_count()},(_,i)=>({
 kind:e.mp_op_kind(i),shape:e.mp_op_shape(i),strokeWidth:e.mp_op_stroke_width(i),
 strokeJoin:e.mp_op_stroke_join(i),miterLimit:e.mp_op_miter_limit(i),
 strokeCap:e.mp_op_stroke_cap(i)
}));
assert.ok(ops.length>rects.length,'clip pop must be preserved in painter order');
const border=ops.find(o=>o.kind===2&&o.strokeWidth>=3);
assert.ok(border,'real Masonry border stroke missing');
assert.equal(rects[border.shape].rgba,encode(225,60,95),'stroke paint color was lost');
assert.equal(border.strokeJoin,0,'red Masonry border must preserve miter join');
const scrollbarStroke=ops.find(o=>o.kind===2&&o.strokeWidth>0&&o.strokeWidth<2&&o.strokeJoin===1);
assert.ok(scrollbarStroke,'scrollbar round-join stroke was silently dropped');
assert.ok(scrollbarStroke.miterLimit>0&&Number.isFinite(scrollbarStroke.miterLimit));
const clipPush=ops.findIndex(o=>o.kind===3);
const clipPop=ops.findIndex(o=>o.kind===4);
assert.ok(clipPush>=0&&clipPop>clipPush,'real Masonry nonisolated clip must retain push/pop order');
const clip=rects[ops[clipPush].shape];
const greenIndex=rects.findIndex(r=>r.rgba===encode(40,170,100));
assert.ok(greenIndex>=0,'clipped Masonry green background missing');
const greenOpIndex=ops.findIndex(o=>o.kind===1&&o.shape===greenIndex);
assert.ok(greenOpIndex>clipPush&&greenOpIndex<clipPop,'clipped draw must remain inside clip scope');
assert.ok(clip.width<rects[greenIndex].width,'viewport must cut green background horizontally');
assert.equal(e.mp_skipped_draws(),0,'unexpected lossy paint ops');
assert.ok(e.mp_glyph_runs()>0,'real Masonry Label emitted no GlyphRun');
assert.ok(e.mp_outlined_glyphs()>=6,'real Parley Cyrillic glyph outlines missing');
const ink=encode(35,36,160);
const outlinedIndexes=rects.flatMap((r,i)=>r.rgba===ink?[i]:[]);
assert.equal(outlinedIndexes.length,e.mp_outlined_glyphs(),
  'outlined glyph paths must correspond to scene shapes');
assert.ok(outlinedIndexes.every(i=>
  e.mp_rect_path_count(i)>=3&&e.mp_rect_path_count(i)<=512),
  'all shaped glyphs must have a bounded vector contour');
console.log('PASS R5a real Masonry GlyphRun→skrifa outline→scene path (Node WASM)');
let depth=0,maxDepth=0,depthAtGreen=0;
for(const op of ops){
  if(op.kind===3){depth++;maxDepth=Math.max(maxDepth,depth);}
  if(op.kind===1&&op.shape===greenIndex)depthAtGreen=depth;
  if(op.kind===4){depth--;assert.ok(depth>=0,'clip popped below zero');}
}
assert.equal(depth,0,'nested Masonry clip stack not balanced');
assert.ok(maxDepth>=2&&depthAtGreen>=2,'two real nested Portals must clip green fill');
const arrowColor=encode(230,50,180);
const arrowIndex=rects.findIndex((r,i)=>r.rgba===arrowColor&&e.mp_rect_path_count(i)>0);
assert.ok(arrowIndex>=0,'actual Masonry DisclosureButton path missing');
const pathCount=e.mp_rect_path_count(arrowIndex);
assert.ok(pathCount>=3&&pathCount<=512);
const pathCommands=Array.from({length:pathCount},(_,j)=>({
 kind:e.mp_rect_path_kind(arrowIndex,j),
 args:Array.from({length:6},(_,k)=>e.mp_rect_path_arg(arrowIndex,j,k))
}));
assert.equal(pathCommands[0].kind,1,'arrow must start with PathEl::MoveTo');
assert.ok(pathCommands.some(x=>x.kind===2),'arrow must contain real PathEl::LineTo');
assert.ok(pathCommands.every(x=>x.args.every(Number.isFinite)));
const matrix=[e.mp_rect_matrix_a(arrowIndex),e.mp_rect_matrix_b(arrowIndex),
 e.mp_rect_matrix_c(arrowIndex),e.mp_rect_matrix_d(arrowIndex),
 e.mp_rect_matrix_e(arrowIndex),e.mp_rect_matrix_f(arrowIndex)];
assert.ok(Math.abs(matrix[1])>0.1&&Math.abs(matrix[2])>0.1,
 'Rust Masonry widget rotation must survive to the WASM transform ABI');
const arrowStroke=ops.find(o=>o.kind===2&&o.shape===arrowIndex);
assert.ok(arrowStroke,'real Masonry arrow must have a stroke');
assert.equal(arrowStroke.strokeCap,1,'open arrow Round caps were not preserved');

const quadColor=encode(10,215,235),cubicColor=encode(240,165,45);
const findCurve=(rgba,kind)=>{
  const i=rects.findIndex((r,index)=>r.rgba===rgba&&
    Array.from({length:e.mp_rect_path_count(index)},(_,j)=>e.mp_rect_path_kind(index,j)).includes(kind));
  assert.ok(i>=0,'Real Rust Masonry Canvas curve missing kind='+kind);
  const n=e.mp_rect_path_count(i);
  assert.equal(n,2,'Each curve is one move and one Bezier segment');
  const p=Array.from({length:n},(_,j)=>({
    kind:e.mp_rect_path_kind(i,j),
    args:Array.from({length:6},(_,k)=>e.mp_rect_path_arg(i,j,k))
  }));
  assert.equal(p[0].kind,1);
  assert.equal(p[1].kind,kind);
  assert.ok(p.every(segment=>segment.args.every(Number.isFinite)));
  const m=[e.mp_rect_matrix_a(i),e.mp_rect_matrix_b(i),e.mp_rect_matrix_c(i),
    e.mp_rect_matrix_d(i),e.mp_rect_matrix_e(i),e.mp_rect_matrix_f(i)];
  assert.ok(m.every(Number.isFinite));
  const op=ops.find(o=>o.kind===2&&o.shape===i);
  assert.ok(op&&op.strokeWidth===5,'Real Canvas curve must be a five-pixel Rust stroke');
  return {i,p,m};
};
const quad=findCurve(quadColor,3),cubic=findCurve(cubicColor,4);
const q=quad.p[1].args,k=cubic.p[1].args;
const quadCenter=[
  (quad.p[0].args[0]+2*q[0]+q[2])/4,
  (quad.p[0].args[1]+2*q[1]+q[3])/4
];
const cubicCenter=[
  (cubic.p[0].args[0]+3*k[0]+3*k[2]+k[4])/8,
  (cubic.p[0].args[1]+3*k[1]+3*k[3]+k[5])/8
];
assert.ok(Math.abs(quadCenter[0]-55)<0.001&&Math.abs(quadCenter[1]-30)<0.001);
assert.ok(Math.abs(cubicCenter[0]-55.25)<0.001&&Math.abs(cubicCenter[1]-14.5)<0.001);
assert.equal(e.mp_skipped_draws(),0,'Bezier curves must not be silently discarded');
// R5b: two real registered OFL font families, explicit WHOLE-RUN coverage.
// These changes use the same live Masonry Label, not a browser CSS substitute.
assert.equal(e.mp_demo_text(),'Привет');
assert.equal(e.mp_demo_font_id(),1);
assert.equal(e.mp_demo_set_text('Rust'),1);
assert.equal(e.mp_demo_font_id(),2,'Latin family was not selected');
assert.equal(e.mp_demo_text(),'Rust');
assert.equal(e.mp_outlined_glyphs(),4);
assert.equal(e.mp_skipped_draws(),0);
for(const text of ['日本','🐱','Привет Rust','Rust猫']){
  assert.equal(e.mp_demo_set_text(text),-6,'unknown coverage not rejected: '+text);
  assert.equal(e.mp_demo_text(),'Rust','rejection mutated accepted text');
  assert.equal(e.mp_outlined_glyphs(),4,'rejection mutated glyph scene');
}
assert.equal(e.mp_demo_set_text(''),-2);
assert.equal(e.mp_demo_set_text('Привет'),1);
assert.equal(e.mp_demo_font_id(),1);
assert.equal(e.mp_outlined_glyphs(),6);
assert.equal(e.mp_skipped_draws(),0);
console.log('PASS R5b registered Latin/Cyrillic font coverage and fail-closed unsupported runs (Node WASM)');
// WASM exports reject out-of-range commands; they never alias a different path.
assert.equal(e.mp_rect_path_kind(quad.i,9999),-1);
assert.ok(Number.isNaN(e.mp_rect_path_arg(cubic.i,9999,0)));
const target=rects.find(r=>r.rgba===brown);
assert.ok(target,'button child surface from Masonry missing');
assert.ok(target.width>0&&target.height>0,'button surface has no layout');
const x=target.x+target.width/2,y=target.y+target.height/2;
assert.equal(e.mp_click_count(),0);
e.mp_pointer(1,x,y);
e.mp_pointer(0,x,y);
assert.equal(e.mp_click_count(),1,'Masonry ButtonPress must be dispatched by RenderRoot');
const oldEvents=e.mp_keyboard_event_count();
assert.equal(e.mp_semantic_button_id(),1,'semantic button identity should be stable');
assert.equal(e.mp_focus_button(0),0);
assert.equal(e.mp_keyboard_button(0,1),-1,'Unfocused button must reject Enter');
assert.equal(e.mp_keyboard_event_count(),oldEvents,'unfocused Enter must not reach Rust');
assert.equal(e.mp_focus_button(1),1);
assert.equal(e.mp_keyboard_button(1,1),1,'Enter down must not activate button');
assert.equal(e.mp_keyboard_button(0,1),2,'Enter up must activate real Masonry ButtonPress');
assert.equal(e.mp_keyboard_button(1,2),2);
assert.equal(e.mp_keyboard_button(0,2),3,'Space up must activate real Masonry ButtonPress');
assert.equal(e.mp_keyboard_button(1,42),-2,'Unknown key must fail closed');
assert.equal(e.mp_keyboard_event_count(),oldEvents+4);
assert.equal(e.mp_focus_button(0),0);
assert.equal(e.mp_keyboard_button(0,1),-1);
assert.equal(e.mp_keyboard_event_count(),oldEvents+4,'blur must fence all scene commands');
assert.ok(e.mp_resize(400,300)>=2,'resize must repaint actual Masonry scene');
assert.deepEqual([e.mp_width(),e.mp_height()],[400,300]);
rects=getRects();
assert.ok(rects.some(r=>r.rgba===blue));
assert.ok(rects.some(r=>r.rgba===brown));
// R4c: JS BigInt must preserve actual Rust WidgetId/AccessKit NodeId across
// disabled updates and change on real Rust widget replacement.
const originalId=e.mp_access_button_id();
assert.equal(typeof originalId,'bigint');
assert.ok(originalId>0n);
assert.equal(e.mp_access_button_disabled(),0);
assert.equal(e.mp_focus_button(1),1);
assert.equal(e.mp_access_focus_id(),originalId);
assert.equal(e.mp_set_button_disabled(1),1);
assert.equal(e.mp_access_button_disabled(),1);
assert.equal(e.mp_access_button_id(),originalId);
assert.equal(e.mp_focused_button(),0);
assert.equal(e.mp_keyboard_button(0,1),-1);
assert.equal(e.mp_set_button_disabled(0),0);
assert.equal(e.mp_access_button_id(),originalId);
assert.equal(e.mp_set_button_present(0),0);
assert.equal(e.mp_access_button_id(),0n);
assert.equal(e.mp_semantic_button_id(),-1);
assert.equal(e.mp_set_button_present(1),1);
const replacementId=e.mp_access_button_id();
assert.ok(replacementId>0n&&replacementId!==originalId);
assert.equal(e.mp_focus_button(1),1);
assert.equal(e.mp_access_focus_id(),replacementId);
// R4d: these strings travel through the actual wasm-bindgen UTF-8 ABI,
 // and are applied by a genuine upstream Masonry TextInput/TextArea.
assert.equal(e.mp_editor_preedit('leak'),-1,'unfocused editor must refuse input');
assert.equal(e.mp_editor_focus(1),1);
assert.equal(e.mp_editor_replace('Привет '),1);
assert.equal(e.mp_editor_committed_text(),'Привет ');
const clicksBeforeIme=e.mp_click_count(),commitsBeforeIme=e.mp_editor_commits();
assert.equal(e.mp_editor_preedit('に'),1);
assert.equal(e.mp_editor_composing(),1);
assert.equal(e.mp_editor_committed_text(),'Привет ',
  'preedit was incorrectly persisted as committed text');
assert.notEqual(e.mp_editor_live_text(),e.mp_editor_committed_text());
assert.equal(e.mp_editor_update_preedit('日本'),1);
assert.equal(e.mp_editor_replace('bad'),-3,'replacement during composition must be rejected');
assert.equal(e.mp_editor_commit('日本'),1);
assert.equal(e.mp_editor_committed_text(),'Привет 日本');
assert.equal(e.mp_editor_commits(),commitsBeforeIme+1);
assert.equal(e.mp_editor_preedit('未確定'),1);
assert.equal(e.mp_editor_cancel(),1);
assert.equal(e.mp_editor_committed_text(),'Привет 日本');
assert.equal(e.mp_editor_live_text(),'Привет 日本');
assert.equal(e.mp_editor_commit('🌍'),1);
assert.equal(e.mp_editor_committed_text(),'Привет 日本🌍');
assert.equal(e.mp_click_count(),clicksBeforeIme,'IME leaked into scene ButtonPress');
assert.ok(e.mp_editor_actions()>=2,'missing genuine Masonry TextAction');
assert.equal(e.mp_editor_focus(0),0);
assert.equal(e.mp_editor_commit('leak'),-1);
// R4e: select UTF-8 byte ranges in the actual TextArea, and use its
// ClipboardPaste routing to edit without reset_text moving the caret to end.
assert.equal(e.mp_editor_focus(1),1);
assert.equal(e.mp_editor_replace('A🌍B日本C'),1);
assert.equal(e.mp_editor_select_range(1,5),1);
const previousActions=e.mp_editor_actions();
assert.equal(e.mp_editor_splice(1,5,'🐱'),1);
assert.equal(e.mp_editor_committed_text(),'A🐱B日本C');
assert.ok(e.mp_editor_actions()>previousActions,'real TextAction missing on splice');
assert.equal(e.mp_editor_splice(2,5,'x'),-4,'invalid emoji byte boundary accepted');
assert.equal(e.mp_editor_splice(5,1,'x'),-4,'reversed UTF-8 byte range accepted');
assert.equal(e.mp_editor_splice(1,5,''),1);
assert.equal(e.mp_editor_committed_text(),'AB日本C');
assert.equal(e.mp_editor_splice(1,1,'ж'),1);
assert.equal(e.mp_editor_committed_text(),'AжB日本C');
assert.equal(e.mp_editor_splice(1,1,'x'.repeat(4097)),-2);
assert.equal(e.mp_editor_preedit('に'),1);
assert.equal(e.mp_editor_splice(1,1,'bad'),-3,'splicing over active IME accepted');
assert.equal(e.mp_editor_cancel(),1);
assert.equal(e.mp_editor_focus(0),0);
assert.equal(e.mp_editor_splice(1,1,'bad'),-1);
console.log('PASS R4e real TextArea selection splice, UTF-8 boundaries, focus/IME fences (Node WASM)');

// R4e2: restore the canonical selected range before the real Ime::Commit.
// The old append-only commit path could produce "Привет X日本猫" under
// Firefox despite a correct DOM selection/replacement.
const source='Привет X日本';
assert.equal(e.mp_editor_focus(1),1);
assert.equal(e.mp_editor_replace(source),1);
const commitCount=e.mp_editor_commits();
assert.equal(e.mp_editor_commit_selection('猫'),-3);
assert.equal(e.mp_editor_begin_composition(13,14),1);
assert.equal(e.mp_editor_update_preedit('猫'),1);
assert.equal(e.mp_editor_committed_text(),source,'preedit must be transient');
assert.equal(e.mp_editor_live_text(),'Привет 猫日本',
  'selection preview must not append');
assert.equal(e.mp_editor_commit_selection('猫'),1);
assert.equal(e.mp_editor_committed_text(),'Привет 猫日本');
assert.equal(e.mp_editor_commits(),commitCount+1);
assert.equal(e.mp_editor_commit_selection('猫'),-3,
  'late compositionend must not commit a second time');
assert.equal(e.mp_editor_committed_text(),'Привет 猫日本');
assert.equal(e.mp_editor_replace(source),1);
assert.equal(e.mp_editor_begin_composition(13,14),1);
assert.equal(e.mp_editor_update_preedit('未確定'),1);
assert.equal(e.mp_editor_cancel(),1);
assert.equal(e.mp_editor_committed_text(),source);
assert.equal(e.mp_editor_live_text(),source,'cancel must restore selection');
assert.equal(e.mp_editor_replace(source),1);
assert.equal(e.mp_editor_begin_composition(13,14),1);
assert.equal(e.mp_editor_update_preedit('仮'),1);
assert.equal(e.mp_editor_focus(0),0);
assert.equal(e.mp_editor_committed_text(),source,'blur must roll back preedit');
assert.equal(e.mp_editor_focus(1),1);
assert.equal(e.mp_editor_begin_composition(14,15),-4,
  'UTF-8 continuation boundary must fail closed');
assert.equal(e.mp_editor_begin_composition(13,14),1);
assert.equal(e.mp_editor_commit_selection('x'.repeat(4097)),-2);
assert.equal(e.mp_editor_cancel(),1);
assert.equal(e.mp_editor_committed_text(),source);
assert.equal(e.mp_click_count(),clicksBeforeIme,'IME text activated the scene');
console.log('PASS R4e2 selected IME commit/rollback, Unicode boundaries and dedupe (Node WASM)');

// R4f: one accepted IME replacement is one bounded undo transaction.
// Transient preedit, IME cancel and recovery must not pollute the journal.
assert.equal(e.mp_editor_replace(source),1);
const historyBaseline=e.mp_editor_undo_depth();
const commitsBaseline=e.mp_editor_commits();
assert.equal(e.mp_editor_begin_composition(13,14),1);
assert.equal(e.mp_editor_update_preedit('未確定'),1);
assert.equal(e.mp_editor_undo(),-3,'undo during composition should fail closed');
assert.equal(e.mp_editor_cancel(),1);
assert.equal(e.mp_editor_committed_text(),source);
assert.equal(e.mp_editor_undo_depth(),historyBaseline);
assert.equal(e.mp_editor_begin_composition(13,14),1);
assert.equal(e.mp_editor_update_preedit('猫'),1);
assert.equal(e.mp_editor_commit_selection('猫'),1);
assert.equal(e.mp_editor_committed_text(),'Привет 猫日本');
assert.equal(e.mp_editor_undo_depth(),historyBaseline+1);
assert.equal(e.mp_editor_commits(),commitsBaseline+1);
assert.equal(e.mp_editor_undo(),1);
assert.equal(e.mp_editor_committed_text(),source);
assert.equal(e.mp_editor_redo_depth(),1);
assert.equal(e.mp_editor_redo(),1);
assert.equal(e.mp_editor_committed_text(),'Привет 猫日本');
assert.equal(e.mp_editor_commits(),commitsBaseline+1,
  'undo/redo must not replay IME commit count');
assert.equal(e.mp_editor_undo(),1);
assert.equal(e.mp_editor_splice(13,14,'ж'),1);
assert.equal(e.mp_editor_committed_text(),'Привет ж日本');
assert.equal(e.mp_editor_redo(),-7,'new edit must invalidate redo branch');
assert.equal(e.mp_editor_focus(0),0);
assert.equal(e.mp_editor_undo(),-1,'unfocused history operations must fail closed');
console.log('PASS R4f bounded Rust TextArea undo/redo over selected IME and cancel without reset (Node WASM)');
