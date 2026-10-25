//! Browser-host experiment using the REAL upstream Masonry RenderRoot.
//! No Akula source and no masonry_winit; no claim of complete renderer support.
#![deny(unsafe_code)]

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
    use masonry::imaging::record::Clip;
    use masonry::dpi::{PhysicalPosition, PhysicalSize};
    use masonry::imaging::record::{Command, Draw, Geometry, Scene};
    use masonry::kurbo::{Join, Point};
    use masonry::layout::{AsUnit, Length};
    use masonry::peniko::{Brush, Color, Fill};
    use masonry::properties::{Background, BorderColor, BorderWidth, CornerRadius, Dimensions};
    use masonry::theme::default_property_set;
    use masonry::ui_events::keyboard::Modifiers;
    use masonry::ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
        PointerType,
    };
    use masonry::widgets::{Button, ButtonPress, Flex, Portal, SizedBox};
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
        /// Local-space bounds preserve arbitrary Affine rotation/skew/scale.
        pub local: [f64; 4],
        /// Canvas2D (a,b,c,d,e,f) matrix from kurbo::Affine.
        pub matrix: [f64; 6],
        /// Clockwise (tl,tr,br,bl), local pixels; all zero means Rect.
        pub radii: [f64; 4],
    }

    #[derive(Debug, Clone, Copy)]
    struct SceneOp {
        // 1=Fill, 2=Stroke, 3=PushClip, 4=PopClip.
        kind: i32,
        shape: i32,
        stroke_width: f64,
        // 0=Miter, 1=Round, 2=Bevel. Shape transport is closed Rect/RoundedRect.
        stroke_join: i32,
        miter_limit: f64,
    }

    fn shape_record(shape: &Geometry, transform: &masonry::kurbo::Affine, color: Color)
        -> Option<SolidRect>
    {
        let (rect, radii) = match shape {
            Geometry::Rect(rect) => (*rect, [0.0;4]),
            Geometry::RoundedRect(round) => {
                let r = round.radii();
                (round.rect(), [r.top_left, r.top_right, r.bottom_right, r.bottom_left])
            }
            Geometry::Path(_) => return None,
        };
        let matrix = transform.as_coeffs();
        let points = [
            *transform * Point::new(rect.x0, rect.y0),
            *transform * Point::new(rect.x1, rect.y0),
            *transform * Point::new(rect.x1, rect.y1),
            *transform * Point::new(rect.x0, rect.y1),
        ];
        let x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let x1 = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let y1 = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        if ![x, y, x1, y1, rect.x0, rect.y0, rect.width(), rect.height()]
            .iter().chain(matrix.iter()).chain(radii.iter()).all(|n| n.is_finite()) {
            return None;
        }
        Some(SolidRect {
            x, y, width:x1-x, height:y1-y,
            rgba:color.to_rgba8().to_u32(),
            local:[rect.x0, rect.y0, rect.width(), rect.height()],
            matrix, radii,
        })
    }

    pub struct BrowserHost {
        root: RenderRoot,
        rects: Vec<SolidRect>,
        ops: Vec<SceneOp>,
        scene_layers: i32,
        accessibility_nodes: i32,
        skipped_draws: i32,
        clicks: Rc<Cell<i32>>,
    }

    impl BrowserHost {
        pub fn new(width: u32, height: u32) -> Self {
            let tile = |color: Color, bordered: bool| {
                let mut props = PropertySet::one(Background::Color(color));
                props.insert(CornerRadius::all(18.px()));
                if bordered {
                    // Actual upstream Masonry pre_paint emits Draw::Stroke.
                    props.insert(BorderColor::new(Color::from_rgb8(225, 60, 95)));
                    props.insert(BorderWidth::all(4.px()));
                }
                NewWidget::new(
                    SizedBox::empty().size(Length::const_px(120.0), Length::const_px(50.0)),
                ).with_props(props)
            };
            // Both colored child surfaces are actual Masonry widgets.
            // The second is inside an actual Button and sends ButtonPress actions.
            let root_widget = Flex::column()
                .with_fixed(tile(Color::from_rgb8(70, 120, 210), true))
                .with_fixed(NewWidget::new(Button::new(tile(Color::from_rgb8(195, 145, 105), false))))
                // A real Portal (80px viewport) clips its 120px-wide green child.
                // This exercises PushClip / Draw / PopClip from Masonry's paint pass.
                .with_fixed(
                    NewWidget::new(Portal::new(tile(Color::from_rgb8(40, 170, 100), false)))
                        .with_props(PropertySet::one(Dimensions::fixed(80.px(), 50.px())))
                )
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
                root, rects: Vec::new(), ops: Vec::new(), scene_layers: 0,
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

            // Exact painter order matters: a canvas drawing or clip can no longer
            // be reconstructed from a list of Rect fills alone.
            let mut flattened = Scene::new();
            plan.replay_into(&mut flattened);
            self.rects.clear();
            self.ops.clear();
            self.skipped_draws = 0;
            let mut clip_support: Vec<bool> = Vec::new();
            let mut unsupported_group_depth = 0usize;
            for command in flattened.commands() {
                match command {
                    Command::PushContext(_) | Command::PopContext => {}
                    Command::PushGroup(_) => {
                        // No group compositing/mask support yet: fail CLOSED.
                        unsupported_group_depth += 1;
                        self.skipped_draws += 1;
                    }
                    Command::PopGroup => {
                        if unsupported_group_depth == 0 { self.skipped_draws += 1; }
                        else { unsupported_group_depth -= 1; }
                    }
                    Command::PushClip(id) => {
                        let supported = if unsupported_group_depth == 0
                            && clip_support.iter().all(|ok| *ok) {
                            match flattened.clip(*id) {
                                Clip::Fill { transform, shape, fill_rule }
                                    if *fill_rule == Fill::NonZero => {
                                    if let Some(record) = shape_record(shape, transform, Color::TRANSPARENT) {
                                        self.add_op(record, 3, 0.0, 0, 4.0);
                                        true
                                    } else { false }
                                }
                                // Stroked clips and even-odd paths need proper path support.
                                _ => false,
                            }
                        } else { false };
                        if !supported { self.skipped_draws += 1; }
                        clip_support.push(supported);
                    }
                    Command::PopClip => {
                        match clip_support.pop() {
                            Some(true) => self.ops.push(SceneOp { kind:4, shape:-1, stroke_width:0.0, stroke_join:0, miter_limit:4.0 }),
                            Some(false) => {},
                            None => self.skipped_draws += 1,
                        }
                    }
                    Command::Draw(id) => {
                        if unsupported_group_depth > 0 || clip_support.iter().any(|ok| !ok) {
                            self.skipped_draws += 1;
                            continue;
                        }
                        let (kind, shape, transform, color, stroke_width, stroke_join, miter_limit) =
                            match flattened.draw_op(*id) {
                                Draw::Fill { shape, transform, brush: Brush::Solid(color), fill_rule: Fill::NonZero, .. } =>
                                    (1, shape, transform, *color, 0.0, 0, 4.0),
                                Draw::Stroke { shape, transform, brush: Brush::Solid(color), stroke, .. }
                                    if stroke.width.is_finite() && stroke.width > 0.0
                                        && stroke.dash_pattern.is_empty()
                                        && stroke.miter_limit.is_finite() && stroke.miter_limit > 0.0 =>
                                {
                                    // A scrollbar's upstream default Stroke::new() has
                                    // Join::Round. Canvas supports Miter, Round and Bevel.
                                    // We only serialize closed Rect/RoundedRect geometry;
                                    // cap style does not affect such a closed stroke.
                                    let join = match stroke.join {
                                        Join::Miter => 0,
                                        Join::Round => 1,
                                        Join::Bevel => 2,
                                    };
                                    (2, shape, transform, *color, stroke.width, join, stroke.miter_limit)
                                },
                                // Every other draw is explicitly counted as unsupported.
                                _ => {
                                    self.skipped_draws += 1;
                                    continue;
                                }
                            };
                        if let Some(record) = shape_record(shape, transform, color) {
                            self.add_op(record, kind, stroke_width, stroke_join, miter_limit);
                        } else { self.skipped_draws += 1; }
                    }
                }
            }
            if !clip_support.is_empty() || unsupported_group_depth != 0 {
                self.skipped_draws += 1;
            }
        }

        fn add_op(&mut self, record: SolidRect, kind: i32, stroke_width: f64,
            stroke_join: i32, miter_limit: f64)
        {
            let idx = self.rects.len() as i32;
            self.rects.push(record);
            self.ops.push(SceneOp { kind, shape: idx, stroke_width, stroke_join, miter_limit });
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

    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_version() -> i32 { 1 }

    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_init(width: u32, height: u32) -> i32 {
        HOST.with(|host| {
            let runtime = BrowserHost::new(width, height);
            let count = runtime.rects.len() as i32;
            *host.borrow_mut() = Some(runtime);
            count
        })
    }

    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_resize(width: u32, height: u32) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut().map(|h| h.resize(width, height)).unwrap_or(-1))
    }

    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_pointer(down: i32, x: f64, y: f64) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut().map(|h| h.pointer(down != 0, x, y)).unwrap_or(-1))
    }

    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_count() -> i32 { get_i32(|h| h.rects.len() as i32) }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_layer_count() -> i32 { get_i32(|h| h.scene_layers) }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_access_nodes() -> i32 { get_i32(|h| h.accessibility_nodes) }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_skipped_draws() -> i32 { get_i32(|h| h.skipped_draws) }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_click_count() -> i32 { get_i32(|h| h.clicks.get()) }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_width() -> i32 { get_i32(|h| h.size().0) }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_height() -> i32 { get_i32(|h| h.size().1) }

    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_x(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.x).unwrap_or(f64::NAN))
    }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_y(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.y).unwrap_or(f64::NAN))
    }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_width(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.width).unwrap_or(f64::NAN))
    }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_height(i: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(|r| r.height).unwrap_or(f64::NAN))
    }
    // SAFETY: ABI function uses only scalar arguments/results and no raw pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_rgba(i: i32) -> u32 {
        HOST.with(|host| host.borrow().as_ref()
            .and_then(|h| h.rects.get(i.max(0) as usize))
            .map(|r| r.rgba).unwrap_or(0))
    }

    // Ordered scene-command ABI. Shape indices refer to mp_rect_* scalar
    // geometry records; PopClip deliberately has no shape.
    // SAFETY: every symbol is scalar-only and passes no borrowed pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_count() -> i32 { get_i32(|h| h.ops.len() as i32) }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_kind(i: i32) -> i32 {
        get_i32(|h| h.ops.get(i.max(0) as usize).map(|o| o.kind).unwrap_or(-1))
    }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_shape(i: i32) -> i32 {
        get_i32(|h| h.ops.get(i.max(0) as usize).map(|o| o.shape).unwrap_or(-1))
    }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_stroke_width(i: i32) -> f64 {
        get_f64(|h| h.ops.get(i.max(0) as usize).map(|o| o.stroke_width).unwrap_or(f64::NAN))
    }

    // SAFETY: scalar-only exported symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_stroke_join(i: i32) -> i32 {
        get_i32(|h| h.ops.get(i.max(0) as usize).map(|o| o.stroke_join).unwrap_or(-1))
    }

    // SAFETY: scalar-only exported symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_miter_limit(i: i32) -> f64 {
        get_f64(|h| h.ops.get(i.max(0) as usize).map(|o| o.miter_limit).unwrap_or(f64::NAN))
    }

    // This is a scalar-only ABI experiment; no raw pointers or shared memory.
    // WASM / Canvas uses these values instead of prematurely rasterizing
    // rotated or rounded geometry into axis-aligned rectangles.
    fn get_rect_f64(i: i32, getter: impl FnOnce(&SolidRect) -> f64) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize).map(getter).unwrap_or(f64::NAN))
    }

    // ABI-exported numeric shape/matrix members (clamped index in accessor).
    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_local_x(i: i32) -> f64 {
        get_rect_f64(i, |r| r.local[0])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_local_y(i: i32) -> f64 {
        get_rect_f64(i, |r| r.local[1])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_local_width(i: i32) -> f64 {
        get_rect_f64(i, |r| r.local[2])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_local_height(i: i32) -> f64 {
        get_rect_f64(i, |r| r.local[3])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_radius_tl(i: i32) -> f64 {
        get_rect_f64(i, |r| r.radii[0])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_radius_tr(i: i32) -> f64 {
        get_rect_f64(i, |r| r.radii[1])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_radius_br(i: i32) -> f64 {
        get_rect_f64(i, |r| r.radii[2])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_radius_bl(i: i32) -> f64 {
        get_rect_f64(i, |r| r.radii[3])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_matrix_a(i: i32) -> f64 {
        get_rect_f64(i, |r| r.matrix[0])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_matrix_b(i: i32) -> f64 {
        get_rect_f64(i, |r| r.matrix[1])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_matrix_c(i: i32) -> f64 {
        get_rect_f64(i, |r| r.matrix[2])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_matrix_d(i: i32) -> f64 {
        get_rect_f64(i, |r| r.matrix[3])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_matrix_e(i: i32) -> f64 {
        get_rect_f64(i, |r| r.matrix[4])
    }

    // SAFETY: no_mangle is confined to this scalar-only ABI symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_matrix_f(i: i32) -> f64 {
        get_rect_f64(i, |r| r.matrix[5])
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
            assert!(h.rects.iter().any(|r| r.rgba == Color::from_rgb8(70,120,210).to_rgba8().to_u32() && r.radii[0] >= 10.0));
            assert!(h.rects.iter().all(|r| r.matrix.iter().all(|n| n.is_finite())));
            assert!(h.rects.iter().any(|r| r.rgba == Color::from_rgb8(195,145,105).to_rgba8().to_u32()));
            assert!(h.ops.iter().any(|o| o.kind == 1), "fill missing");
            assert!(h.ops.iter().any(|o| o.kind == 2 && o.stroke_width >= 3.0 && o.stroke_join == 0), "miter border stroke missing");
            assert!(h.ops.iter().any(|o| o.kind == 2 && o.stroke_join == 1), "round-join scrollbar stroke missing");
            assert!(h.ops.iter().any(|o| o.kind == 3), "clip push missing");
            assert!(h.ops.iter().any(|o| o.kind == 4), "clip pop missing");
            assert_eq!(h.skipped_draws, 0, "lossy scene operations in R2 demo");
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
