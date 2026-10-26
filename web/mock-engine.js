// Only for unit-testing the native browser ABI; never a production engine.
(function(){
 const initial={x:160,y:120,width:250,height:170};
 let r={...initial},selected=false,doc=0,scene=0,undo=[];
 const clamp=(x,a,b)=>Math.max(a,Math.min(b,x));
 function change(next){
  if(Object.keys(r).every(k=>r[k]===next[k]))return 0;
  if(undo.length===32)undo.shift();
  undo.push({...r});r=next;doc++;scene++;return 2;
 }
 globalThis.__uiPlatformMockEngine={
  uip_version:()=>1,
  uip_select_at(x,y){const n=x>=r.x&&x<r.x+r.width&&y>=r.y&&y<r.y+r.height;if(selected===n)return 0;selected=n;scene++;return 1;},
  uip_move_to(x,y){return selected?change({...r,x:clamp(x,0,900-r.width),y:clamp(y,0,500-r.height)}):0;},
  uip_set_width(n){return selected?change({...r,width:clamp(n,30,900-r.x)}):0;},
  uip_set_height(n){return selected?change({...r,height:clamp(n,30,500-r.y)}):0;},
  uip_undo(){if(!undo.length)return 0;r=undo.pop();doc++;scene++;return 2;},
  uip_reset(){const d=change({...initial});const was=selected;selected=false;if(was&&!d)scene++;return d||(was?1:0);},
  uip_get_x:()=>r.x,uip_get_y:()=>r.y,uip_get_width:()=>r.width,uip_get_height:()=>r.height,
  uip_is_selected:()=>selected?1:0,uip_document_revision:()=>doc,uip_scene_revision:()=>scene
 };
})();
