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
    use masonry::core::{NewWidget, PropertySet, Widget, WidgetTag, WindowEvent};
    use masonry::imaging::record::Clip;
    use masonry::dpi::{PhysicalPosition, PhysicalSize};
    use masonry::imaging::{Painter, record::{Command, Draw, Geometry, Scene}};
    use masonry::kurbo::{Affine, BezPath, Cap, Join, PathEl, Point, Shape as _, Stroke, Vec2};
    use masonry::layout::{AsUnit, Length};
    use masonry::peniko::{Brush, Color, Fill};
    use masonry::properties::{Background, BorderColor, BorderWidth, ContentColor, CornerRadius, Dimensions};
    use masonry::theme::default_property_set;
    use masonry::ui_events::keyboard::Modifiers;
    use masonry::ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
        PointerType,
    };
    use masonry::widgets::{Button, ButtonPress, Canvas, DisclosureButton, Flex, Portal, SizedBox};
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
        // A bounded vector of actual Kurbo path commands, empty for Rect/RoundedRect.
        pub path: Vec<PathCommand>,
    }

    #[derive(Debug, Clone, Copy)]
    struct PathCommand {
        // 1=MoveTo, 2=LineTo, 3=QuadTo, 4=CurveTo, 5=ClosePath.
        kind: i32,
        args: [f64; 6],
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
        // Closed rects ignore caps; open paths use this start/end cap.
        stroke_cap: i32,
    }

    fn shape_record(shape: &Geometry, transform: &masonry::kurbo::Affine, color: Color)
        -> Option<SolidRect>
    {
        let (rect, radii, path) = match shape {
            Geometry::Rect(rect) => (*rect, [0.0; 4], Vec::new()),
            Geometry::RoundedRect(round) => {
                let r = round.radii();
                (round.rect(), [r.top_left, r.top_right, r.bottom_right, r.bottom_left], Vec::new())
            }
            Geometry::Path(bez) => {
                // Avoid exporting unbounded geometry to the browser through scalar getters.
                if bez.elements().is_empty() || bez.elements().len() > 512 { return None; }
                let mut path = Vec::with_capacity(bez.elements().len());
                for segment in bez.elements() {
                    let mut args = [0.0_f64; 6];
                    let kind = match *segment {
                        PathEl::MoveTo(p) => { args[..2].copy_from_slice(&[p.x,p.y]); 1 },
                        PathEl::LineTo(p) => { args[..2].copy_from_slice(&[p.x,p.y]); 2 },
                        PathEl::QuadTo(a,b) => {
                            args[..4].copy_from_slice(&[a.x,a.y,b.x,b.y]); 3
                        }
                        PathEl::CurveTo(a,b,c) => {
                            args.copy_from_slice(&[a.x,a.y,b.x,b.y,c.x,c.y]); 4
                        }
                        PathEl::ClosePath => 5,
                    };
                    if args.iter().any(|n| !n.is_finite()) { return None; }
                    path.push(PathCommand { kind, args });
                }
                (bez.bounding_box(), [0.0;4], path)
            }
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
            matrix, radii, path,
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
        curve_tag: WidgetTag<Canvas>,
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
            // The tagged disclosure arrow is a REAL Masonry BezPath stroke.
            // Applying an affine widget transform tests the full matrix pipeline.
            let arrow_tag: WidgetTag<DisclosureButton> = WidgetTag::unique();
            let curve_tag: WidgetTag<Canvas> = WidgetTag::unique();
            let root_widget = Flex::column()
                .with_fixed(tile(Color::from_rgb8(70, 120, 210), true))
                .with_fixed(NewWidget::new(Button::new(tile(Color::from_rgb8(195, 145, 105), false))))
                // A real Portal (80px viewport) clips its 120px-wide green child.
                // This exercises PushClip / Draw / PopClip from Masonry's paint pass.
                .with_fixed(
                    NewWidget::new(Portal::new(
                        NewWidget::new(Portal::new(tile(Color::from_rgb8(40, 170, 100), false)))
                            .with_props(PropertySet::one(Dimensions::fixed(80.px(), 50.px())))
                    ))
                    .with_props(PropertySet::one(Dimensions::fixed(65.px(), 50.px())))
                )
                // Keep the 80px-tall curve widget within the initial 240px
                // browser canvas: place it BESIDE the 64px disclosure arrow.
                .with_fixed(Flex::row()
                    .with_fixed(
                        NewWidget::new(DisclosureButton::new(false))
                            .with_tag(arrow_tag)
                            .with_props(
                                PropertySet::new()
                                    .with(Dimensions::fixed(64.px(),64.px()))
                                    .with(ContentColor::new(Color::from_rgb8(230,50,180)))
                            )
                    )
                    .with_fixed(
                        NewWidget::new(Canvas::default())
                            .with_tag(curve_tag)
                            .with_props(PropertySet::one(Dimensions::fixed(120.px(),80.px())))
                    )
                    .prepare()
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
            // Transform only this existing Rust widget, never the Canvas JS shape.
            let center = Vec2::new(32.0,32.0);
            let matrix = Affine::translate(center)
                .then_rotate(0.45)
                .then_translate(-center);
            root.edit_widget_with_tag(arrow_tag, |mut arrow| arrow.set_transform(matrix));
            // This is an upstream Masonry::Canvas widget. Painter stores both
            // Bezier paths in its real retained Scene; JS never fabricates them.
            root.edit_widget_with_tag(curve_tag, |mut curve| {
                Canvas::update_scene(&mut curve, |_ctx, scene, _size| {
                    let mut painter = Painter::new(scene);
                    let mut quadratic = BezPath::new();
                    quadratic.move_to((8.0, 60.0));
                    quadratic.quad_to((55.0, 0.0), (102.0, 60.0));
                    painter.stroke(&quadratic, &Stroke::new(5.0),
                        Color::from_rgb8(10, 215, 235)).draw();
                    let mut cubic = BezPath::new();
                    cubic.move_to((8.0, 70.0));
                    cubic.curve_to((30.0, -4.0), (80.0, -4.0), (104.0, 70.0));
                    painter.stroke(&cubic, &Stroke::new(5.0),
                        Color::from_rgb8(240, 165, 45)).draw();
                });
            });
            root.handle_window_event(WindowEvent::EnableAccessTree);
            let mut host = Self {
                root, rects: Vec::new(), ops: Vec::new(), scene_layers: 0,
                accessibility_nodes: 0, skipped_draws: 0, clicks, curve_tag,
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
                                        self.add_op(record, 3, 0.0, 0, 4.0, 0);
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
                            Some(true) => self.ops.push(SceneOp { kind:4, shape:-1, stroke_width:0.0, stroke_join:0, miter_limit:4.0, stroke_cap:0 }),
                            Some(false) => {},
                            None => self.skipped_draws += 1,
                        }
                    }
                    Command::Draw(id) => {
                        if unsupported_group_depth > 0 || clip_support.iter().any(|ok| !ok) {
                            self.skipped_draws += 1;
                            continue;
                        }
                        let (kind, shape, transform, color, stroke_width, stroke_join, miter_limit, stroke_cap) =
                            match flattened.draw_op(*id) {
                                Draw::Fill { shape, transform, brush: Brush::Solid(color), fill_rule: Fill::NonZero, .. } =>
                                    (1, shape, transform, *color, 0.0, 0, 4.0, 0),
                                Draw::Stroke { shape, transform, brush: Brush::Solid(color), stroke, .. }
                                    if stroke.width.is_finite() && stroke.width > 0.0
                                        && stroke.dash_pattern.is_empty()
                                        && stroke.miter_limit.is_finite() && stroke.miter_limit > 0.0 =>
                                {
                                    // A scrollbar's upstream default Stroke::new() has
                                    // Join::Round. Canvas supports Miter, Round and Bevel.
                                    // We only serialize closed Rect/RoundedRect geometry;
                                    // cap style does not affect such a closed stroke.
                                    if matches!(shape, Geometry::Path(_))
                                        && stroke.start_cap != stroke.end_cap {
                                        self.skipped_draws += 1;
                                        continue;
                                    }
                                    let cap = match stroke.start_cap {
                                        Cap::Butt => 0,
                                        Cap::Round => 1,
                                        Cap::Square => 2,
                                    };
                                    let join = match stroke.join {
                                        Join::Miter => 0,
                                        Join::Round => 1,
                                        Join::Bevel => 2,
                                    };
                                    (2, shape, transform, *color, stroke.width, join, stroke.miter_limit, cap)
                                },
                                // Every other draw is explicitly counted as unsupported.
                                _ => {
                                    self.skipped_draws += 1;
                                    continue;
                                }
                            };
                        if let Some(record) = shape_record(shape, transform, color) {
                            self.add_op(record, kind, stroke_width, stroke_join, miter_limit, stroke_cap);
                        } else { self.skipped_draws += 1; }
                    }
                }
            }
            if !clip_support.is_empty() || unsupported_group_depth != 0 {
                self.skipped_draws += 1;
            }
        }

        fn add_op(&mut self, record: SolidRect, kind: i32, stroke_width: f64,
            stroke_join: i32, miter_limit: f64, stroke_cap: i32)
        {
            let idx = self.rects.len() as i32;
            self.rects.push(record);
            self.ops.push(SceneOp { kind, shape: idx, stroke_width, stroke_join, miter_limit, stroke_cap });
        }

        #[cfg(test)]
        fn inject_oversized_path_for_fidelity_test(&mut self) {
            // This tests the ACTUAL RenderRoot->Scene serializer, not just
            // shape_record in isolation: a >512 PathEl draw MUST be omitted
            // and counted as unsupported, not approximated as its bounding box.
            self.root.edit_widget_with_tag(self.curve_tag, |mut curve| {
                Canvas::update_scene(&mut curve, |_ctx, scene, _size| {
                    let mut path = BezPath::new();
                    path.move_to((5.0, 20.0));
                    for i in 0..513 {
                        path.line_to((5.0 + (i % 100) as f64, 20.0));
                    }
                    Painter::new(scene).stroke(&path, &Stroke::new(5.0),
                        Color::from_rgb8(10, 215, 235)).draw();
                });
            });
            self.redraw();
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

    // SAFETY: scalar-only exported symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_op_stroke_cap(i: i32) -> i32 {
        get_i32(|h| h.ops.get(i.max(0) as usize).map(|o| o.stroke_cap).unwrap_or(-1))
    }

    // SAFETY: scalar-only exported symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_path_count(i: i32) -> i32 {
        get_i32(|h| h.rects.get(i.max(0) as usize).map(|r| r.path.len() as i32).unwrap_or(-1))
    }

    // SAFETY: scalar-only exported symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_path_kind(i: i32, j: i32) -> i32 {
        get_i32(|h| h.rects.get(i.max(0) as usize)
            .and_then(|r| r.path.get(j.max(0) as usize))
            .map(|p| p.kind).unwrap_or(-1))
    }

    // SAFETY: scalar-only exported symbol.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_rect_path_arg(i: i32, j: i32, k: i32) -> f64 {
        get_f64(|h| h.rects.get(i.max(0) as usize)
            .and_then(|r| r.path.get(j.max(0) as usize))
            .and_then(|p| p.args.get(k.max(0) as usize))
            .copied().unwrap_or(f64::NAN))
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
        fn rejects_invalid_or_oversized_path_in_real_geometry_serializer() {
            let white = Color::from_rgb8(255,255,255);
            let empty = BezPath::new();
            assert!(shape_record(&Geometry::Path(empty), &Affine::IDENTITY, white).is_none());

            let mut nan = BezPath::new();
            nan.move_to((0.0, 1.0));
            nan.quad_to((f64::NAN, 2.0), (5.0, 4.0));
            assert!(shape_record(&Geometry::Path(nan), &Affine::IDENTITY, white).is_none());

            let mut at_limit = BezPath::new();
            at_limit.move_to((0.0, 0.0));
            for i in 0..511 {
                at_limit.line_to((i as f64, 1.0));
            }
            let accepted = shape_record(&Geometry::Path(at_limit.clone()), &Affine::IDENTITY, white)
                .expect("512 path commands must be supported");
            assert_eq!(accepted.path.len(),512);
            at_limit.line_to((513.0, 1.0));
            assert!(shape_record(&Geometry::Path(at_limit), &Affine::IDENTITY, white).is_none(),
                "513 PathEl must fail closed instead of drawing a bounding box");
        }

        #[test]
        fn real_renderroot_rejects_oversized_path_without_drawing_it() {
            let mut h = BrowserHost::new(320,240);
            assert_eq!(h.skipped_draws,0);
            h.inject_oversized_path_for_fidelity_test();
            assert!(h.skipped_draws > 0,"missing unsupported-path diagnostic");
            let cyan = Color::from_rgb8(10,215,235).to_rgba8().to_u32();
            assert!(!h.rects.iter().any(|r| r.rgba == cyan),
                "an unsupported 513-element path leaked into the painter stream");
            assert!(h.rects.iter().any(|r|
                r.rgba == Color::from_rgb8(70,120,210).to_rgba8().to_u32()),
                "valid independent widgets must still be retained");
        }

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
            assert_eq!(h.skipped_draws, 0, "lossy scene operations in R3 demo");
            let mut depth: i32 = 0;
            let mut peak: i32 = 0;
            let mut green_depth: i32 = 0;
            for op in &h.ops {
                if op.kind == 3 { depth += 1; peak = peak.max(depth); }
                if op.kind == 1 && op.shape >= 0
                    && h.rects[op.shape as usize].rgba
                        == Color::from_rgb8(40,170,100).to_rgba8().to_u32()
                { green_depth = depth; }
                if op.kind == 4 { depth -= 1; assert!(depth>=0); }
            }
            assert_eq!(depth,0,"unbalanced nested clip scopes");
            assert!(peak>=2 && green_depth>=2,"green fill must sit inside two real Portal clips");
            let arrow_color = Color::from_rgb8(230,50,180).to_rgba8().to_u32();
            let arrow = h.rects.iter().find(|r| r.rgba==arrow_color && !r.path.is_empty())
                .expect("real Masonry DisclosureButton BezPath stroke missing");
            assert!(arrow.path.len()>=3);
            assert!(arrow.matrix[1].abs()>0.1 && arrow.matrix[2].abs()>0.1,
                "Rust rotated widget transform was flattened");
            assert!(h.ops.iter().any(|op| op.kind==2 && op.stroke_cap==1
                && op.shape>=0 && h.rects[op.shape as usize].rgba==arrow_color),
                "open arrow stroke must preserve round caps");
            let cyan = Color::from_rgb8(10,215,235).to_rgba8().to_u32();
            let amber = Color::from_rgb8(240,165,45).to_rgba8().to_u32();
            assert!(h.rects.iter().any(|r| r.rgba == cyan
                && r.path.iter().any(|command| command.kind == 3)),
                "actual Masonry Canvas QuadTo curve was not serialized");
            assert!(h.rects.iter().any(|r| r.rgba == amber
                && r.path.iter().any(|command| command.kind == 4)),
                "actual Masonry Canvas CurveTo curve was not serialized");
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
