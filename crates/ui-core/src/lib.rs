#![forbid(unsafe_code)]
pub const SURFACE_WIDTH: i32 = 900;
pub const SURFACE_HEIGHT: i32 = 500;
pub const MIN_SIZE: i32 = 30;
pub const INITIAL_RECT: Rect = Rect { x: 160, y: 120, width: 250, height: 170 };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect { pub x: i32, pub y: i32, pub width: i32, pub height: i32 }
impl Rect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiCommand { SelectAt { x: i32, y: i32 }, MoveTo { x: i32, y: i32 }, SetWidth(i32), SetHeight(i32), Undo, Reset }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet { pub document_changed: bool, pub scene_changed: bool, pub selection_changed: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot { pub rect: Rect, pub selected: bool, pub document_revision: u64, pub scene_revision: u64 }
pub struct UiModel { rect: Rect, selected: bool, document_revision: u64, scene_revision: u64, undo_stack: Vec<Rect> }
impl Default for UiModel {
    fn default() -> Self {
        Self { rect: INITIAL_RECT, selected: false, document_revision: 0, scene_revision: 0, undo_stack: Vec::new() }
    }
}
impl UiModel {
    pub fn snapshot(&self) -> Snapshot { Snapshot { rect: self.rect, selected: self.selected, document_revision: self.document_revision, scene_revision: self.scene_revision } }
    fn geometry(&mut self, rect: Rect) -> ChangeSet {
        if rect == self.rect { return ChangeSet::default(); }
        if self.undo_stack.len() == 32 { self.undo_stack.remove(0); }
        self.undo_stack.push(self.rect);
        self.rect = rect;
        self.document_revision += 1;
        self.scene_revision += 1;
        ChangeSet { document_changed: true, scene_changed: true, selection_changed: false }
    }
    pub fn dispatch(&mut self, command: UiCommand) -> ChangeSet {
        match command {
            UiCommand::SelectAt { x, y } => {
                let selected = self.rect.contains(x, y);
                if selected == self.selected { return ChangeSet::default(); }
                self.selected = selected;
                self.scene_revision += 1;
                ChangeSet { scene_changed: true, selection_changed: true, ..ChangeSet::default() }
            }
            UiCommand::MoveTo { x, y } if self.selected => self.geometry(Rect {
                x: x.clamp(0, SURFACE_WIDTH - self.rect.width),
                y: y.clamp(0, SURFACE_HEIGHT - self.rect.height), ..self.rect
            }),
            UiCommand::SetWidth(width) if self.selected => {
                let width = width.clamp(MIN_SIZE, SURFACE_WIDTH - self.rect.x);
                self.geometry(Rect { width, ..self.rect })
            }
            UiCommand::SetHeight(height) if self.selected => {
                let height = height.clamp(MIN_SIZE, SURFACE_HEIGHT - self.rect.y);
                self.geometry(Rect { height, ..self.rect })
            }
            UiCommand::Undo => {
                let Some(rect) = self.undo_stack.pop() else { return ChangeSet::default(); };
                self.rect = rect;
                self.document_revision += 1;
                self.scene_revision += 1;
                ChangeSet { document_changed: true, scene_changed: true, selection_changed: false }
            }
            UiCommand::Reset => {
                let change = self.geometry(INITIAL_RECT);
                let was_selected = self.selected;
                self.selected = false;
                if was_selected && !change.scene_changed { self.scene_revision += 1; }
                ChangeSet { document_changed: change.document_changed, scene_changed: change.scene_changed || was_selected, selection_changed: was_selected }
            }
            _ => ChangeSet::default()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn selection_is_not_document_revision() {
        let mut m=UiModel::default();
        let change=m.dispatch(UiCommand::SelectAt{x:200,y:200});
        assert!(change.selection_changed);
        assert_eq!(m.snapshot().document_revision,0);
        assert_eq!(m.snapshot().scene_revision,1);
    }
    #[test] fn no_op_and_undo() {
        let mut m=UiModel::default();
        assert_eq!(m.dispatch(UiCommand::MoveTo{x:300,y:300}),ChangeSet::default());
        m.dispatch(UiCommand::SelectAt{x:200,y:200});
        assert_eq!(m.dispatch(UiCommand::MoveTo{x:160,y:120}),ChangeSet::default());
        m.dispatch(UiCommand::MoveTo{x:10000,y:-3});
        assert_eq!(m.snapshot().rect.x,650);
        assert_eq!(m.snapshot().rect.y,0);
        m.dispatch(UiCommand::Undo);
        assert_eq!(m.snapshot().rect,INITIAL_RECT);
        assert_eq!(m.snapshot().document_revision,2);
    }
    #[test] fn resize_and_reset() {
        let mut m=UiModel::default();
        m.dispatch(UiCommand::SelectAt{x:200,y:200});
        m.dispatch(UiCommand::SetWidth(-2));
        assert_eq!(m.snapshot().rect.width,MIN_SIZE);
        m.dispatch(UiCommand::Reset);
        assert_eq!(m.snapshot().rect,INITIAL_RECT);
        assert!(!m.snapshot().selected);
    }
}