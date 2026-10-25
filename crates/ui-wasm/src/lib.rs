use rui_core::{UiCommand,UiModel};
use std::cell::RefCell;
thread_local! { static MODEL: RefCell<UiModel> = RefCell::new(UiModel::default()); }
fn command(c: UiCommand) -> i32 {
    MODEL.with(|m| { let d=m.borrow_mut().dispatch(c); if d.document_changed {2} else if d.scene_changed {1} else {0} })
}
#[no_mangle] pub extern "C" fn uip_version() -> i32 {1}
#[no_mangle] pub extern "C" fn uip_select_at(x:i32,y:i32)->i32 {command(UiCommand::SelectAt{x,y})}
#[no_mangle] pub extern "C" fn uip_move_to(x:i32,y:i32)->i32 {command(UiCommand::MoveTo{x,y})}
#[no_mangle] pub extern "C" fn uip_set_width(n:i32)->i32 {command(UiCommand::SetWidth(n))}
#[no_mangle] pub extern "C" fn uip_set_height(n:i32)->i32 {command(UiCommand::SetHeight(n))}
#[no_mangle] pub extern "C" fn uip_undo()->i32 {command(UiCommand::Undo)}
#[no_mangle] pub extern "C" fn uip_reset()->i32 {command(UiCommand::Reset)}
#[no_mangle] pub extern "C" fn uip_get_x()->i32 {MODEL.with(|m|m.borrow().snapshot().rect.x)}
#[no_mangle] pub extern "C" fn uip_get_y()->i32 {MODEL.with(|m|m.borrow().snapshot().rect.y)}
#[no_mangle] pub extern "C" fn uip_get_width()->i32 {MODEL.with(|m|m.borrow().snapshot().rect.width)}
#[no_mangle] pub extern "C" fn uip_get_height()->i32 {MODEL.with(|m|m.borrow().snapshot().rect.height)}
#[no_mangle] pub extern "C" fn uip_is_selected()->i32 {MODEL.with(|m|i32::from(m.borrow().snapshot().selected))}
#[no_mangle] pub extern "C" fn uip_document_revision()->i32 {MODEL.with(|m|m.borrow().snapshot().document_revision as i32)}
#[no_mangle] pub extern "C" fn uip_scene_revision()->i32 {MODEL.with(|m|m.borrow().snapshot().scene_revision as i32)}
