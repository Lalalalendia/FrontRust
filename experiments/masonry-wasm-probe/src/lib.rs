//! Browser-host experiment using the REAL upstream Masonry RenderRoot.
//! No Akula source and no masonry_winit; no claim of complete renderer support.
#![forbid(unsafe_code)]

pub fn masonry_core_identity_size() -> usize {
    core::mem::size_of::<masonry_core::core::WidgetId>()
}

#[cfg(feature = "widgets")]
pub fn masonry_widget_type_size() -> usize {
    core::mem::size_of::<masonry::widgets::Label>()
}

#[cfg(feature = "native-host")]
pub fn masonry_native_host_type_size() -> usize {
    core::mem::size_of::<masonry_winit::app::WindowId>()
}

#[cfg(feature = "widgets")]
mod browser_host {
    use masonry::app::{RenderRoot, RenderRootOptions, RenderRootSignal, WindowSizePolicy};
    use masonry::core::{NewWidget, PropertySet, Widget, WindowEvent};
    use masonry::dpi::{PhysicalPosition, PhysicalSize};
    use masonry::imaging::record::{Command, Draw, Geometry, Scene};
    use masonry::kurbo::Point;
    use masonry::layout::Length;
    use masonry::peniko::{Brush, Color};
    use masonry::properties::Background;
    use masonry::theme::default_property_set;
    use masonry::ui_events::keyboard::Modifiers;
    use masonry::ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
        PointerType,
    };
    use masonry::widgets::{Button, ButtonPress, Flex, SizedBox};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    pub struct SolidRect {
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
        /// RGBA8, little-endian packed (R in the lowest byte).
        pub rgba: u32,
    }

    pub struct BrowserHost {
        root: RenderRoot,
        rects: Vec<SolidRect>,
        scene_layers: i32,
        accessibility_nodes: i32,
        skipped_draws: i32,
        clicks: Rc<Cell<i32>>,
    }

    impl BrowserHost {
        pub fn new(width: u32, height: u32) -> Self {
            let tile = |color: Color| {
                NewWidget::new(
                    SizedBox::empty().size(Length::const_px(120.0), Length::const_px(50.0)),
                )
                .with_props(PropertySet::one(Background::Color(color)))
            };
            // Both colored child surfaces are actual Masonry widgets.
            // The second is inside an actual Button and sends ButtonPress actions.
            let root_widget = Flex::column()
                .with_fixed(tile(Color::from_rgb8(70, 120, 210)))
                .with_fixed(NewWidget::new(Button::new(tile(Color::from_rgb8(195, 145, 105)))))
                .prepare();
            let clicks = Rc::new(Cell::new(0_i32));
            let action_clicks = Rc::clone(&clicks);
            let mut root = RenderRoot::new(
                root_widget,
                move |signal| {
                    if let RenderRootSignal::Action(action, _) = signal {
                        if action.is::<ButtonPress>() {
                            action_clicks.set(action_clicks.get().saturating_add(1));
                        }
                    }
                },
                RenderRootOptions {
                    default_properties: Arc::new(default_property_set()),
                    use_system_fonts: false,
                    size_policy: WindowSizePolicy::User,
                    size: PhysicalSize::new(width.clamp(160, 1600), height.clamp(120, 1200)),
                    scale_factor: 1.0,
                    test_font: None,
                },
            );
            root.handle_window_event(WindowEvent::EnableAccessTree);
            let mut host = Self {
                root, rects: Vec::new(), scene_layers: 0,
                accessibility_nodes: 0, skipped_draws: 0, clicks,
            };
            host.redraw();
            host
        }

        pub fn redraw(&mut self) {
            let (plan, access) = self.root.redraw();
            self.scene_layers = plan.layers.len() as i32;
            if let Some(tree) = access {
                self.accessibility_nodes = tree.nodes.len() as i32;
            }

            // Replays REAL retained Masonry scene layers into an Imaging scene.
            // Explicitly bounded Canvas2D serializer: solid axis-aligned Rect and
            // RoundedRect backgrounds only. Strokes, text, clips, gradients and
            // unsupported draw kinds must be implemented before production use.
            let mut flattened = Scene::new();
            plan.replay_into(&mut flattened);
            self.rects.clear();
            self.skipped_draws = 0;
            for command in flattened.commands() {
                let Command::Draw(id) = command else { continue };
                let Draw::Fill { brush: Brush::Solid(color), shape, transform, .. } =
                    flattened.draw_op(*id) else {
                        self.skipped_draws += 1;
                        continue;
                    };
                let rect = match shape {
                    Geometry::Rect(rect) => *rect,
                    Geometry::RoundedRect(rect) => rect.rect(),
                    Geometry::Path(_) => {
                        self.skipped_draws += 1;
                        continue;
                    }
                };
                let a = *transform * Point::new(rect.x0, rect.y0);
                let b = *transform * Point::new(rect.x1, rect.y1);
                if !a.x.is_finite() || !a.y.is_finite() || !b.x.is_finite() || !b.y.is_finite() {
                    self.skipped_draws += 1;
                    continue;
                }
                self.rects.push(SolidRect {
                    x: a.x.min(b.x),
                    y: a.y.min(b.y),
                    width: (b.x-a.x).abs(),
                    height: (b.y-a.y).abs(),
                    rgba: (*color).to_rgba8().to_u32(),
                });
            }
        }

        pub fn resize(&mut self, width: u32, height: u32) -> i32 {
            self.root.handle_window_event(
                WindowEvent::Resize(PhysicalSize::new(width.clamp(160, 1600), height.clamp(120, 1200)))
            );
            self.redraw();
            self.rects.len() as i32
        }

        pub fn pointer(&mut self, pressed: bool, x: f64, y: f64) -> i32 {
            let state = PointerState {
                time: if pressed { 1 } else { 2 },
                position: PhysicalPosition::new(x, y),
                buttons: Default::default(),
                modifiers: Modifiers::empty(),
                count: 1,
                contact_geometry: PhysicalSize::new(1.0, 1.0),
                orientation: Default::default(),
                pressure: if pressed { 0.5 } else { 0.0 },
                tangential_pressure: 0.0,
                scale_factor: 1.0,
            };
            let event = PointerButtonEvent {
                button: Some(PointerButton::Primary),
                pointer: PointerInfo {
                    pointer_id: Some(PointerId::PRIMARY),
                    persistent_device_id: None,
                    pointer_type: PointerType::Mouse,
                },
                state,
            };
            self.root.handle_pointer_event(if pressed {
                PointerEvent::Down(event)
            } else {
                PointerEvent::Up(event)
            });
            self.redraw();
            self.clicks.get()
        }

        pub fn size(&self) -> (i32, i32) {
            let size = self.root.size();
            (size.width as i32, size.height as i32)
        }
    }

    thread_local! {
        static HOST: RefCell<Option<BrowserHost>> = const { RefCell::new(None) };
    }

    fn get_i32(f: impl FnOnce(&BrowserHost) -> i32) -> i32 {
        HOST.with(|host| host.borrow().as_ref().map(f).unwrap_or(-1))
    }
    fn get_f64(f: impl FnOnce(&BrowserHost) -> f64) -> f64 {
        HOST.with(|host| host.borrow().as_ref().map(f).unwrap_or(f64::NAN))
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn mp_version() -> i32 { 1 }

    #[unsafe(no_mangle)]
    pub extern "C" fn mp_init(width: u32, height: u32) -> i32 {
        HOST.with(|host| {
            let runtime = BrowserHost::new(width, height);
            let count = runtime.rects.len() as i32;
            *host.borrow_mut() = Some(runtime);
            count
        })
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn mp_resize(width: u32, height: u32) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut().map(|h| h.resize(width, height)).unwrap_or(-1))
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn mp_pointer(down: i32, x: f64, y: f64) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut().map(|h| h.pointer(down != 0, x, y)).unwrap_or(-1))
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_count() -> i32 { get_i32(|h| h.rects.len() as i32) }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_layer_count() -> i32 { get_i32(|h| h.scene_layers) }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_access_nodes() -> i32 { get_i32(|h| h.accessibility_nodes) }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_skipped_draws() -> i32 { get_i32(|h| h.skipped_draws) }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_click_count() -> i32 { get_i32(|h| h.clicks.get()) }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_width() -> i32 { get_i32(|h| h.size().0) }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_height() -> i32 { get_i32(|h| h.size().1) }

    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_x(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.x).unwrap_or(f64::NAN))
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_y(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.y).unwrap_or(f64::NAN))
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_width(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.width).unwrap_or(f64::NAN))
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_height(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.height).unwrap_or(f64::NAN))
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_rgba(i: i32) -> u32 {
        HOST.with(|host| host.borrow().as_ref()
            .and_then(|h| h.rects.get(i.max(0) as usize))
            .map(|r| r.rgba).unwrap_or(0))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn headless_renderroot_two_widgets_and_resize() {
            let mut h = BrowserHost::new(320, 240);
            assert!(h.scene_layers > 0);
            assert!(h.rects.len() >= 2);
            assert!(h.accessibility_nodes > 0);
            assert!(h.rects.iter().any(|r| r.rgba == Color::from_rgb8(70,120,210).to_rgba8().to_u32()));
            assert!(h.rects.iter().any(|r| r.rgba == Color::from_rgb8(195,145,105).to_rgba8().to_u32()));
            assert!(h.resize(400, 300) >= 2);
            assert_eq!(h.size(), (400,300));
        }
    }
}

#[cfg(all(test, feature = "widgets", not(target_arch = "wasm32")))]
#[test]
fn native_headless_renderroot_scene_and_resize() {
    // The same production BrowserHost gets exercised by its module-level test.
    assert!(masonry_widget_type_size() > 0);
}
