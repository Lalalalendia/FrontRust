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
    use masonry::accesskit::{Action as AccessAction, NodeId as AccessNodeId, Role as AccessRole};
    use masonry::app::{RenderRoot, RenderRootOptions, RenderRootSignal, WindowSizePolicy};
    use masonry::core::{Ime, NewWidget, PropertySet, TextEvent, Widget, WidgetTag, WindowEvent};
    use masonry::imaging::record::Clip;
    use masonry::dpi::{PhysicalPosition, PhysicalSize};
    use masonry::imaging::{Painter, record::{Command, Draw, Geometry, GlyphRun, Scene}};
    use masonry::kurbo::{Affine, BezPath, Cap, Join, PathEl, Point, Shape as _, Stroke, Vec2};
    use masonry::layout::{AsUnit, Length};
    use masonry::peniko::{Blob, Brush, Color, Fill};
    use masonry::parley::style::{FontFamily, FontFamilyName, StyleProperty};
    use masonry::properties::{Background, BorderColor, BorderWidth, ContentColor, CornerRadius, Dimensions};
    use masonry::theme::default_property_set;
    use masonry::ui_events::keyboard::{Key, Modifiers, NamedKey};
    use masonry::ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
        PointerType,
    };
    use masonry::widgets::{Button, ButtonPress, Canvas, DisclosureButton, Flex, Label, Portal, SizedBox, TextAction, TextArea, TextInput};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;
    use skrifa::{FontRef, GlyphId, MetadataProvider,
        instance::{LocationRef, Size}, outline::{DrawSettings, OutlinePen}};
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen::prelude::wasm_bindgen;

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

    #[derive(Default)]
    struct GlyphPath(BezPath);
    // Shaped glyph IDs and positions come from the REAL Masonry GlyphRun.
    // These are OpenType y-up paths, reflected to Canvas y-down at draw time.
    impl OutlinePen for GlyphPath {
        fn move_to(&mut self, x: f32, y: f32) { self.0.move_to((x as f64, y as f64)); }
        fn line_to(&mut self, x: f32, y: f32) { self.0.line_to((x as f64, y as f64)); }
        fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
            self.0.quad_to((cx as f64, cy as f64), (x as f64, y as f64));
        }
        fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32,
            x: f32, y: f32) {
            self.0.curve_to((cx0 as f64, cy0 as f64),
                (cx1 as f64, cy1 as f64), (x as f64, y as f64));
        }
        fn close(&mut self) { self.0.close_path(); }
    }

    // R5a: only small, solid, unhinted, unvaried monochrome outlines.
    // No fillText fallback: that would discard upstream shaping and offsets.
    fn outlined_glyph_records(run: &GlyphRun) -> Option<Vec<SolidRect>> {
        if run.glyphs.len() > 32 || run.font.data.len() > 2_000_000
            || !run.font_size.is_finite() || run.font_size <= 0.0
            || run.font_size > 128.0 || run.glyph_transform.is_some()
            || run.brush_transform.is_some() || run.hint
            || !run.normalized_coords.is_empty()
            || run.font_embolden != Vec2::ZERO
            || run.composite != Default::default()
            || !matches!(&run.style, masonry::peniko::Style::Fill(Fill::NonZero)) {
            return None;
        }
        let Brush::Solid(color) = &run.brush else { return None; };
        let font = FontRef::from_index(run.font.data.as_ref(), run.font.index).ok()?;
        let outlines = font.outline_glyphs();
        let mut records = Vec::new();
        for glyph in &run.glyphs {
            if !glyph.x.is_finite() || !glyph.y.is_finite() { return None; }
            let outline = outlines.get(GlyphId::new(glyph.id))?;
            let mut pen = GlyphPath::default();
            outline.draw(
                DrawSettings::unhinted(Size::new(run.font_size), LocationRef::default()),
                &mut pen
            ).ok()?;
            if pen.0.elements().is_empty() { continue; } // e.g. space
            let xform = run.transform
                * Affine::translate((glyph.x as f64, glyph.y as f64))
                * Affine::scale_non_uniform(1.0, -1.0);
            records.push(shape_record(&Geometry::Path(pen.0), &xform, *color)?);
        }
        Some(records)
    }

    // The pinned Parley TextArea needs glyph clusters to turn UTF-8 byte
    // offsets into cursors. Without ANY font, select_byte_range silently
    // snaps both cursors to EOF: the next paste appends instead of replacing.
    //
    // A tiny generated OFL-free in-house synthetic OpenType fixture is used
    // exclusively for deterministic R4e text-layout tests. Its glyphs are
    // identical placeholder boxes, NOT production typography.
    fn r4e_layout_font() -> Blob<u8> {
        let mut bytes=Vec::new();
        for line in include_str!("../fixtures/r4e-layout-font.hex").split_ascii_whitespace() {
            assert!(line.len()%2==0);
            for i in (0..line.len()).step_by(2) {
                bytes.push(u8::from_str_radix(&line[i..i+2],16)
                    .expect("invalid synthetic R4e font hex"));
            }
        }
        Blob::new(Arc::new(bytes))
    }

    #[derive(Debug, Clone)]
    struct EditorEdit {
        start: usize,
        removed: String,
        inserted: String,
    }

    pub struct BrowserHost {
        root: RenderRoot,
        // A separate genuine Masonry TextInput RenderRoot. Its glyph drawing
        // is intentionally NOT passed to the R3b limited Canvas2D transport:
        // native DOM input supplies text presentation/OS IME behavior.
        text_root: RenderRoot,
        editor_tag: WidgetTag<TextArea<true>>,
        editor_committed: String,
        editor_composing: bool,
        editor_preedit: String,
        editor_composition_range: Option<(usize, usize)>,
        editor_commits: i32,
        editor_actions: Rc<Cell<i32>>,
        undo_stack: Vec<EditorEdit>,
        redo_stack: Vec<EditorEdit>,
        history_muted: bool,
        rects: Vec<SolidRect>,
        ops: Vec<SceneOp>,
        scene_layers: i32,
        accessibility_nodes: i32,
        accessible_button: bool,
        access_button_id: Option<u64>,
        access_button_disabled: bool,
        access_focus_id: u64,
        skipped_draws: i32,
        glyph_runs: i32,
        outlined_glyphs: i32,
        clicks: Rc<Cell<i32>>,
        curve_tag: WidgetTag<Canvas>,
        button_tag: WidgetTag<Button>,
        root_tag: WidgetTag<Flex>,
        keyboard_events: i32,
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
            // Stable semantic identity of the demo's one real keyboard-focusable button.
            let button_tag: WidgetTag<Button> = WidgetTag::named("frontrust-primary-action");
            let root_tag: WidgetTag<Flex> = WidgetTag::named("frontrust-demo-root");
            let root_widget = Flex::column()
                .with_fixed(tile(Color::from_rgb8(70, 120, 210), true))
                .with_fixed(
                    NewWidget::new(Button::new(tile(Color::from_rgb8(195, 145, 105), false)))
                        .with_tag(button_tag)
                )
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
                .with_fixed(
                    NewWidget::new(
                        Label::new("Привет")
                            .with_style(StyleProperty::FontFamily(FontFamily::Single(
                                FontFamilyName::Named("FrontRust Layout Fixture".into())
                            )))
                            .with_hint(false)
                    ).with_props(PropertySet::one(
                        ContentColor::new(Color::from_rgb8(35, 36, 160))
                    ))
                )
                .prepare().with_tag(root_tag);
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
                    test_font: Some(r4e_layout_font()),
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
            // The real TextArea is nested in the real TextInput; tag the
            // focusable TextArea, not its generic-container wrapper.
            let editor_tag: WidgetTag<TextArea<true>> = WidgetTag::named("frontrust-r4d-editor");
            let editor_actions = Rc::new(Cell::new(0_i32));
            let count_actions = Rc::clone(&editor_actions);
            // RenderRootOptions.test_font only appends the fixture to the
            // Latin fallback chain. Cyrillic, Han and emoji need an explicit
            // family or Parley may snap their byte-index cursors to EOF.
            // Font family matches name ID 1 in r4e-layout-font.hex.
            let editor = TextArea::new_editable("").with_style(
                StyleProperty::FontFamily(FontFamily::Single(
                    FontFamilyName::Named("FrontRust Layout Fixture".into()),
                )),
            );
            let text_widget = NewWidget::new(TextInput::from_text_area(
                NewWidget::new(editor).with_tag(editor_tag)
            ));
            let mut text_root = RenderRoot::new(
                text_widget,
                move |signal| {
                    if let RenderRootSignal::Action(action, _) = signal {
                        if action.is::<TextAction>() {
                            count_actions.set(count_actions.get().saturating_add(1));
                        }
                    }
                },
                RenderRootOptions {
                    default_properties: Arc::new(default_property_set()),
                    use_system_fonts: false,
                    size_policy: WindowSizePolicy::User,
                    size: PhysicalSize::new(320, 48),
                    scale_factor: 1.0,
                    // A layout-only fixture ensures cursor clusters exist in
                    // WASM/CI where system font discovery is unavailable.
                    test_font: Some(r4e_layout_font()),
                },
            );
            text_root.handle_window_event(WindowEvent::EnableAccessTree);
            let _ = text_root.redraw();
            let mut host = Self {
                root, text_root, editor_tag, editor_committed: String::new(),
                editor_composing: false, editor_preedit: String::new(),
                editor_composition_range: None,
                editor_commits: 0, editor_actions,
                undo_stack: Vec::new(), redo_stack: Vec::new(),
                history_muted: false,
                rects: Vec::new(), ops: Vec::new(), scene_layers: 0,
                accessibility_nodes: 0, accessible_button: false,
                access_button_id: None, access_button_disabled: false, access_focus_id: 0,
                skipped_draws: 0, glyph_runs: 0, outlined_glyphs: 0,
                clicks, curve_tag, button_tag, root_tag,
                keyboard_events: 0,
            };
            host.redraw();
            host
        }

        pub fn redraw(&mut self) {
            // TreeUpdate is a delta: preserve the previous semantic state when
            // subsequent frames omit unchanged nodes.
            let button_node_id = self.root.get_widget_with_tag(self.button_tag)
                .map(|widget| AccessNodeId::from(widget.id()));
            // Removal/recreation changes the real WidgetId. Never reuse stale
            // accessibility metadata from the previous instance.
            if self.access_button_id != button_node_id.map(|id| id.0) {
                self.access_button_id = None;
                self.accessible_button = false;
                self.access_button_disabled = false;
            }
            let (plan, access) = self.root.redraw();
            self.scene_layers = plan.layers.len() as i32;
            if let Some(tree) = access {
                self.accessibility_nodes = tree.nodes.len() as i32;
                self.access_focus_id = tree.focus.0;
                if let Some(target_id) = button_node_id {
                    for (node_id, node) in &tree.nodes {
                        if *node_id == target_id {
                            self.access_button_id = Some(target_id.0);
                            self.accessible_button = node.role() == AccessRole::Button
                                && node.supports_action(AccessAction::Click);
                            self.access_button_disabled = node.is_disabled();
                        }
                    }
                }
            }

            // Exact painter order matters: a canvas drawing or clip can no longer
            // be reconstructed from a list of Rect fills alone.
            let mut flattened = Scene::new();
            plan.replay_into(&mut flattened);
            self.rects.clear();
            self.ops.clear();
            self.skipped_draws = 0;
            self.glyph_runs = 0;
            self.outlined_glyphs = 0;
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
                        if let Draw::GlyphRun(run) = flattened.draw_op(*id) {
                            let Some(outlines) = outlined_glyph_records(run) else {
                                self.skipped_draws += 1; // unsupported glyph format, NEVER invisible
                                continue;
                            };
                            if self.rects.len() + outlines.len() > 128
                                || self.ops.len() + outlines.len() > 256 {
                                self.skipped_draws += 1; // bounded transport
                                continue;
                            }
                            self.glyph_runs += 1;
                            self.outlined_glyphs += outlines.len() as i32;
                            for path in outlines { self.add_op(path, 1, 0.0, 0, 4.0, 0); }
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

        // A semantic projection is offered only when the actual tagged
        // AccessKit node declares Button plus Click. Do not infer it from
        // the existence of a Rust widget or an arbitrary count of nodes.
        pub fn semantic_button_id(&self) -> i32 {
            if self.accessible_button && self.root.get_widget_with_tag(self.button_tag).is_some() {
                1
            } else {
                -1
            }
        }

        pub fn access_button_id(&self) -> u64 {
            if self.semantic_button_id() == 1 {
                self.access_button_id.unwrap_or(0)
            } else {
                0
            }
        }

        pub fn access_button_disabled(&self) -> i32 {
            if self.access_button_id() == 0 { -1 }
            else { i32::from(self.access_button_disabled) }
        }

        // Modify an actual Rust widget, then observe the updated AccessKit
        // node in the *next* RenderRoot accessibility pass.
        pub fn set_button_disabled(&mut self, disabled: bool) -> i32 {
            if self.root.get_widget_with_tag(self.button_tag).is_none() { return -1; }
            if disabled { self.root.focus_on(None); }
            self.root.edit_widget_with_tag(self.button_tag, |mut button| {
                button.ctx.set_disabled(disabled);
            });
            self.redraw();
            self.access_button_disabled()
        }

        // Remove/reinsert the real Rust Button at its original Flex index.
        // Keeping a spacer avoids changing the independent R3b drawing layout.
        pub fn set_button_present(&mut self, present: bool) -> i32 {
            let was_present = self.root.get_widget_with_tag(self.button_tag).is_some();
            if was_present == present { return i32::from(present); }
            if !present {
                self.root.focus_on(None);
                self.root.edit_widget_with_tag(self.root_tag, |mut root| {
                    Flex::set_fixed_spacer(&mut root, 1, Length::const_px(50.0));
                });
            } else {
                // A removed tag may remain reserved within Masonry's widget
                // registry. A new widget gets both a new WidgetTag and NodeId.
                let tag: WidgetTag<Button> = WidgetTag::unique();
                self.button_tag = tag;
                let child = NewWidget::new(
                    SizedBox::empty().size(Length::const_px(120.0), Length::const_px(50.0))
                ).with_props(
                    PropertySet::one(Background::Color(Color::from_rgb8(195, 145, 105)))
                        .with(CornerRadius::all(18.px()))
                );
                self.root.edit_widget_with_tag(self.root_tag, |mut root| {
                    Flex::set_fixed(&mut root, 1, NewWidget::new(Button::new(child)).with_tag(tag));
                });
            }
            self.redraw();
            i32::from(self.semantic_button_id() == 1)
        }

        // Focus only the actual tagged Masonry Button. The DOM mirror is
        // a semantic keyboard/accessibility projection, not a second action.
        pub fn focus_button(&mut self, should_focus: bool) -> i32 {
            let id = if should_focus {
                self.root.get_widget_with_tag(self.button_tag).map(|w| w.id())
            } else {
                None
            };
            if should_focus && (id.is_none() || self.access_button_disabled) { return -1; }
            if !self.root.focus_on(id) { return -1; }
            self.redraw();
            self.focused_button()
        }

        pub fn focused_button(&self) -> i32 {
            let id = self.root.get_widget_with_tag(self.button_tag).map(|w| w.id());
            if id.is_some() && self.root.focused_widget() == id { 1 } else { 0 }
        }

        // 1=Enter; 2=Space. Reject all other keys, and do not route commands
        // without Rust focus. KeyUp triggers the upstream ButtonPress.
        pub fn keyboard_button(&mut self, down: bool, code: i32) -> i32 {
            if self.focused_button() != 1 { return -1; }
            let key = match code {
                1 => Key::Named(NamedKey::Enter),
                2 => Key::Character(" ".into()),
                _ => return -2,
            };
            self.keyboard_events = self.keyboard_events.saturating_add(1);
            let event = if down { TextEvent::key_down(key) }
                else { TextEvent::key_up(key) };
            self.root.handle_text_event(event);
            self.redraw();
            self.clicks.get()
        }

        // PlainEditor::text() deliberately EXCLUDES IME preedit. This is
        // the genuine committed document model from the upstream TextArea.
        pub fn editor_document_text(&self) -> String {
            self.text_root.get_widget_with_tag(self.editor_tag)
                .map(|area| area.inner().text().to_string())
                .unwrap_or_default()
        }

        // Preview the provisional IME selection against the last accepted
        // document, not Parley's mutable transient preedit buffer.
        pub fn editor_live_text(&self) -> String {
            let mut preview = self.editor_committed.clone();
            if self.editor_composing {
                if let Some((start, end)) = self.editor_composition_range {
                    if start <= end && end <= preview.len()
                        && preview.is_char_boundary(start) && preview.is_char_boundary(end) {
                        preview.replace_range(start..end, &self.editor_preedit);
                        return preview;
                    }
                }
                preview.push_str(&self.editor_preedit);
            }
            preview
        }

        pub fn editor_focused(&self) -> i32 {
            let id = self.text_root.get_widget_with_tag(self.editor_tag)
                .map(|area| area.id());
            i32::from(id.is_some() && self.text_root.focused_widget() == id)
        }

        pub fn editor_focus(&mut self, focused: bool) -> i32 {
            if !focused && self.editor_composing {
                let cancel = self.editor_cancel();
                if cancel != 1 { return cancel; }
            }
            let id = if focused {
                self.text_root.get_widget_with_tag(self.editor_tag).map(|area| area.id())
            } else { None };
            if focused && id.is_none() { return -1; }
            if !self.text_root.focus_on(id) { return -1; }
            if focused {
                self.text_root.handle_text_event(TextEvent::Ime(Ime::Enabled));
            }
            let _ = self.text_root.redraw();
            self.editor_focused()
        }

        fn editor_accepts(&self, text: &str) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if text.len() > 4096 { return -2; }
            if self.editor_composing { return -3; }
            1
        }

        // All accepted full-value replacement also passes through a genuine
        // TextArea selection and ClipboardPaste, without a full editor reset.
        pub fn editor_replace(&mut self, text: &str) -> i32 {
            let accepted = self.editor_accepts(text);
            if accepted < 0 { return accepted; }
            let len = self.editor_document_text().len();
            self.editor_splice(0, len, text)
        }

        fn record_edit(&mut self, start: usize, removed: String, inserted: String) {
            if self.history_muted || removed == inserted { return; }
            self.undo_stack.push(EditorEdit { start, removed, inserted });
            if self.undo_stack.len() > 64 { self.undo_stack.remove(0); }
            self.redo_stack.clear();
        }

        // Standalone IME commits have no externally provided range. Use
        // Unicode-scalar boundaries for one canonical text transition.
        fn record_text_transition(&mut self, before: &str, after: &str) {
            let mut prefix = 0;
            for (a, b) in before.chars().zip(after.chars()) {
                if a != b { break; }
                prefix += a.len_utf8();
            }
            let mut suffix = 0;
            for (a, b) in before[prefix..].chars().rev()
                .zip(after[prefix..].chars().rev()) {
                if a != b { break; }
                suffix += a.len_utf8();
            }
            self.record_edit(
                prefix,
                before[prefix..before.len()-suffix].to_owned(),
                after[prefix..after.len()-suffix].to_owned(),
            );
        }

        fn editor_restore_snapshot(&mut self) -> i32 {
            let canonical = self.editor_committed.clone();
            let current = self.editor_document_text();
            if current == canonical { return 1; }
            // Compensating TextArea splice restores ONLY differing bytes,
            // not the entire editor widget. This internal repair is omitted
            // from the app-level undo journal.
            let mut prefix = 0;
            for (a, b) in current.chars().zip(canonical.chars()) {
                if a != b { break; }
                prefix += a.len_utf8();
            }
            let mut suffix = 0;
            for (a, b) in current[prefix..].chars().rev()
                .zip(canonical[prefix..].chars().rev()) {
                if a != b { break; }
                suffix += a.len_utf8();
            }
            let old_muted = self.history_muted;
            self.history_muted = true;
            let result = self.editor_splice(
                prefix, current.len() - suffix,
                &canonical[prefix..canonical.len() - suffix],
            );
            self.history_muted = old_muted;
            if result != 1 || self.editor_document_text() != canonical { return -5; }
            1
        }

        pub fn editor_undo(&mut self) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if self.editor_composing { return -3; }
            let Some(edit) = self.undo_stack.pop() else { return -7; };
            let Some(end) = edit.start.checked_add(edit.inserted.len()) else {
                self.undo_stack.push(edit); return -4;
            };
            if self.editor_document_text().get(edit.start..end)
                != Some(edit.inserted.as_str()) {
                self.undo_stack.push(edit); return -5;
            }
            let old_muted = self.history_muted;
            self.history_muted = true;
            let result = self.editor_splice(edit.start, end, &edit.removed);
            self.history_muted = old_muted;
            if result != 1 { self.undo_stack.push(edit); return result; }
            self.redo_stack.push(edit);
            1
        }

        pub fn editor_redo(&mut self) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if self.editor_composing { return -3; }
            let Some(edit) = self.redo_stack.pop() else { return -7; };
            let Some(end) = edit.start.checked_add(edit.removed.len()) else {
                self.redo_stack.push(edit); return -4;
            };
            if self.editor_document_text().get(edit.start..end)
                != Some(edit.removed.as_str()) {
                self.redo_stack.push(edit); return -5;
            }
            let old_muted = self.history_muted;
            self.history_muted = true;
            let result = self.editor_splice(edit.start, end, &edit.inserted);
            self.history_muted = old_muted;
            if result != 1 { self.redo_stack.push(edit); return result; }
            self.undo_stack.push(edit);
            1
        }

        // R4e: arbitrary UTF-8 byte selection in the *real* upstream
        // TextArea, without resetting its editor/cursor state.
        // Reject UTF-8 continuation bytes before calling the Masonry API,
        // whose invalid-boundary behavior is an intentional no-op.
        pub fn editor_select_range(&mut self, start: usize, end: usize) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if self.editor_composing { return -3; }
            let document = self.editor_document_text();
            if start > end || end > document.len()
                || !document.is_char_boundary(start)
                || !document.is_char_boundary(end) { return -4; }
            let tag = self.editor_tag;
            self.text_root.edit_widget_with_tag(tag, |mut area| {
                TextArea::select_byte_range(&mut area, start, end);
            });
            let _ = self.text_root.redraw();
            1
        }

        // Dispatch text to upstream TextArea with its selected byte range.
        // ClipboardPaste uses the real editor's insert_or_replace_selection,
        // whereas the old reset_text path always moved the cursor to the end.
        pub fn editor_splice(&mut self, start: usize, end: usize, inserted: &str) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if self.editor_composing { return -3; }
            let before = self.editor_document_text();
            if start > end || end > before.len()
                || !before.is_char_boundary(start)
                || !before.is_char_boundary(end) { return -4; }
            let Some(new_len) = before.len().checked_sub(end-start)
                .and_then(|len| len.checked_add(inserted.len())) else { return -2; };
            if new_len > 4096 { return -2; }
            let expected = format!("{}{}{}", &before[..start], inserted, &before[end..]);
            let select = self.editor_select_range(start, end);
            if select != 1 { return select; }
            let before_actions = self.editor_actions.get();
            self.text_root.handle_text_event(TextEvent::ClipboardPaste(inserted.to_owned()));
            let _ = self.text_root.redraw();
            let actual = self.editor_document_text();
            if actual != expected {
                // Record the real Rust TextArea result in CI: -5 on its own
                // hides whether a stale selection or an unhandled paste failed.
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("R4e TextArea splice mismatch: before={before:?}, range=({start},{end}), inserted={inserted:?}, expected={expected:?}, actual={actual:?}");
                return -5;
            }
            if expected != before && self.editor_actions.get() <= before_actions {
                return -6; // Never silently accept a bypass of Masonry TextAction.
            }
            self.editor_committed = actual;
            if expected != before {
                self.record_edit(start, before[start..end].to_owned(), inserted.to_owned());
            }
            1
        }

        // R4e2: capture the authoritative UTF-8 selection before the
        // browser's composition events collapse/change the native caret.
        pub fn editor_begin_composition(&mut self, start: usize, end: usize) -> i32 {
            let selected = self.editor_select_range(start, end);
            if selected != 1 { return selected; }
            let started = self.editor_preedit("");
            if started == 1 { self.editor_composition_range = Some((start, end)); }
            started
        }

        pub fn editor_preedit(&mut self, text: &str) -> i32 {
            let accepted = self.editor_accepts(text);
            if accepted < 0 { return accepted; }
            self.editor_composition_range = None;
            self.text_root.handle_text_event(TextEvent::Ime(
                Ime::Preedit(text.to_owned(), None)
            ));
            self.editor_composing = true;
            self.editor_preedit = text.to_owned();
            let _ = self.text_root.redraw();
            1
        }

        pub fn editor_update_preedit(&mut self, text: &str) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if text.len() > 4096 { return -2; }
            if !self.editor_composing { return -3; }
            self.text_root.handle_text_event(TextEvent::Ime(
                Ime::Preedit(text.to_owned(), None)
            ));
            self.editor_preedit = text.to_owned();
            let _ = self.text_root.redraw();
            1
        }

        pub fn editor_commit(&mut self, text: &str) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            let before = self.editor_committed.clone();
            if before.len().saturating_add(text.len()) > 4096 { return -2; }
            self.text_root.handle_text_event(TextEvent::Ime(
                Ime::Commit(text.to_owned())
            ));
            self.editor_composing = false;
            self.editor_preedit.clear();
            self.editor_composition_range = None;
            let _ = self.text_root.redraw();
            let actual = self.editor_document_text();
            if actual.len() > 4096 { return -2; }
            self.record_text_transition(&before, &actual);
            self.editor_committed = actual;
            self.editor_commits = self.editor_commits.saturating_add(1);
            1
        }

        // Parley temporarily replaces the selected range with preedit.
        // Restore only changed bytes before reapplying genuine IME Commit;
        // internal recovery must not create a separate history transaction.
        pub fn editor_commit_selection(&mut self, inserted: &str) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            if !self.editor_composing { return -3; }
            let Some((start, end)) = self.editor_composition_range else { return -3; };
            let before = self.editor_committed.clone();
            if start > end || end > before.len()
                || !before.is_char_boundary(start) || !before.is_char_boundary(end) {
                return -4;
            }
            let Some(next_size) = before.len().checked_sub(end - start)
                .and_then(|n| n.checked_add(inserted.len())) else { return -2; };
            if next_size > 4096 { return -2; }
            let expected = format!("{}{}{}", &before[..start], inserted, &before[end..]);
            self.text_root.handle_text_event(TextEvent::Ime(Ime::Disabled));
            self.editor_composing = false;
            self.editor_preedit.clear();
            self.editor_composition_range = None;
            let restored = self.editor_restore_snapshot();
            if restored != 1 { return restored; }
            let selected = self.editor_select_range(start, end);
            if selected != 1 { return selected; }
            let actions_before = self.editor_actions.get();
            self.text_root.handle_text_event(TextEvent::Ime(
                Ime::Commit(inserted.to_owned())
            ));
            let _ = self.text_root.redraw();
            let actual = self.editor_document_text();
            if actual != expected || self.editor_actions.get() <= actions_before {
                let _ = self.editor_restore_snapshot();
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("R4f IME selection mismatch: before={before:?} range=({start},{end}) inserted={inserted:?} expected={expected:?} actual={actual:?}");
                return -5;
            }
            self.editor_committed = actual;
            self.record_edit(start, before[start..end].to_owned(), inserted.to_owned());
            self.editor_commits = self.editor_commits.saturating_add(1);
            1
        }

        pub fn editor_cancel(&mut self) -> i32 {
            if self.editor_focused() != 1 { return -1; }
            self.text_root.handle_text_event(TextEvent::Ime(Ime::Disabled));
            self.text_root.handle_text_event(TextEvent::Ime(Ime::Enabled));
            self.editor_composing = false;
            self.editor_preedit.clear();
            self.editor_composition_range = None;
            let restored = self.editor_restore_snapshot();
            let _ = self.text_root.redraw();
            if restored != 1 || self.editor_document_text() != self.editor_committed { return -5; }
            1
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

    // SAFETY: all ABI exports pass only scalar values and no pointers.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_semantic_button_id() -> i32 { get_i32(|h| h.semantic_button_id()) }

    // AccessKit NodeId is a 64-bit identity: WASM JS must preserve it as
    // BigInt, never truncate to a floating-point numeric DOM ID.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_access_button_id() -> u64 {
        HOST.with(|host| host.borrow().as_ref().map(|h| h.access_button_id()).unwrap_or(0))
    }

    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_access_button_disabled() -> i32 {
        get_i32(|h| h.access_button_disabled())
    }

    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_access_focus_id() -> u64 {
        HOST.with(|host| host.borrow().as_ref().map(|h| h.access_focus_id).unwrap_or(0))
    }

    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_set_button_disabled(disabled: i32) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut()
            .map(|h| h.set_button_disabled(disabled != 0)).unwrap_or(-1))
    }

    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_set_button_present(present: i32) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut()
            .map(|h| h.set_button_present(present != 0)).unwrap_or(-1))
    }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_focus_button(focus: i32) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut()
            .map(|h| h.focus_button(focus != 0)).unwrap_or(-1))
    }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_focused_button() -> i32 { get_i32(|h| h.focused_button()) }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_keyboard_button(down: i32, key_code: i32) -> i32 {
        HOST.with(|host| host.borrow_mut().as_mut()
            .map(|h| h.keyboard_button(down != 0, key_code)).unwrap_or(-1))
    }

    // SAFETY: scalar ABI.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_keyboard_event_count() -> i32 { get_i32(|h| h.keyboard_events) }

    // R4d UTF-8 strings cross JS↔Rust using wasm-bindgen's checked
    // string marshalling, not manual unsafe pointers or lossy u32 packing.
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_replace(value: String) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_replace(&value)).unwrap_or(-1))
    }
    // Selection uses the old scalar ABI; dynamic Unicode insertion uses
    // the generated wasm-bindgen String wrapper.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_select_range(start: u32, end: u32) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_select_range(start as usize, end as usize))
            .unwrap_or(-1))
    }
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_splice(start: u32, end: u32, inserted: String) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_splice(start as usize, end as usize, &inserted))
            .unwrap_or(-1))
    }

    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_begin_composition(start: u32, end: u32) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_begin_composition(start as usize, end as usize))
            .unwrap_or(-1))
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_commit_selection(value: String) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_commit_selection(&value))
            .unwrap_or(-1))
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_preedit(value: String) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_preedit(&value)).unwrap_or(-1))
    }
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_update_preedit(value: String) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_update_preedit(&value)).unwrap_or(-1))
    }
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_commit(value: String) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_commit(&value)).unwrap_or(-1))
    }
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_committed_text() -> String {
        HOST.with(|h| h.borrow().as_ref()
            .map(|host| host.editor_committed.clone()).unwrap_or_default())
    }
    #[cfg(target_arch = "wasm32")]
    #[allow(unsafe_code)]
    #[wasm_bindgen]
    pub fn mp_editor_live_text() -> String {
        HOST.with(|h| h.borrow().as_ref()
            .map(|host| host.editor_live_text()).unwrap_or_default())
    }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_focus(focused: i32) -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_focus(focused != 0)).unwrap_or(-1))
    }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_focused() -> i32 { get_i32(|h| h.editor_focused()) }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_composing() -> i32 { get_i32(|h| i32::from(h.editor_composing)) }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_commits() -> i32 { get_i32(|h| h.editor_commits) }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_actions() -> i32 { get_i32(|h| h.editor_actions.get()) }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_cancel() -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_cancel()).unwrap_or(-1))
    }

    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_undo() -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_undo()).unwrap_or(-1))
    }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_redo() -> i32 {
        HOST.with(|h| h.borrow_mut().as_mut()
            .map(|host| host.editor_redo()).unwrap_or(-1))
    }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_undo_depth() -> i32 {
        get_i32(|h| h.undo_stack.len() as i32)
    }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_editor_redo_depth() -> i32 {
        get_i32(|h| h.redo_stack.len() as i32)
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
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_glyph_runs() -> i32 { get_i32(|h| h.glyph_runs) }
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "C" fn mp_outlined_glyphs() -> i32 { get_i32(|h| h.outlined_glyphs) }
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
        fn r5a_real_masonry_label_produces_bounded_outline_paths() {
            let mut h = BrowserHost::new(320, 360);
            assert_eq!(h.skipped_draws, 0, "glyphs must not disappear silently");
            assert!(h.glyph_runs > 0, "no genuine Masonry GlyphRun detected");
            assert!(h.outlined_glyphs >= 6, "expected Cyrillic label outlines");
            let ink = Color::from_rgb8(35, 36, 160).to_rgba8().to_u32();
            let glyphs = h.rects.iter().filter(|r| r.rgba == ink).collect::<Vec<_>>();
            assert_eq!(glyphs.len(), h.outlined_glyphs as usize);
            assert!(glyphs.iter().all(|r| r.path.len() >= 3
                && r.path.len() <= 512 && r.path.iter().all(|p|
                    p.args.iter().all(|n| n.is_finite()))));
            assert!(glyphs.iter().any(|r| r.y > 120.0 && r.y < 360.0));
            h.resize(400, 300);
            assert_eq!(h.skipped_draws, 0);
            assert!(h.outlined_glyphs >= 6);
        }

        #[test]
        fn real_renderroot_rejects_oversized_path_without_drawing_it() {
            let mut h = BrowserHost::new(320,360);
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
        fn r4f_real_textarea_journal_undo_redo_and_ime_without_reset() {
            let mut h = BrowserHost::new(320, 360);
            assert_eq!(h.editor_undo(), -1, "unfocused undo must fail closed");
            assert_eq!(h.editor_focus(true), 1);
            assert_eq!(h.editor_undo(), -7, "empty history must be a no-op");
            assert_eq!(h.editor_replace("Привет X日本"), 1);
            let initial = h.undo_stack.len();
            assert_eq!(h.editor_begin_composition(13, 14), 1);
            assert_eq!(h.editor_update_preedit("猫"), 1);
            assert_eq!(h.editor_undo(), -3, "undo during IME composition must fail");
            assert_eq!(h.editor_cancel(), 1);
            assert_eq!(h.editor_document_text(), "Привет X日本");
            assert_eq!(h.undo_stack.len(), initial,
                "canceled preedit/rollback must not enter undo history");
            assert_eq!(h.editor_begin_composition(13, 14), 1);
            assert_eq!(h.editor_update_preedit("猫"), 1);
            assert_eq!(h.editor_commit_selection("猫"), 1);
            assert_eq!(h.undo_stack.len(), initial + 1,
                "committed selected IME must be ONE history transaction");
            assert_eq!(h.editor_committed, "Привет 猫日本");
            let commits = h.editor_commits;
            assert_eq!(h.editor_undo(), 1);
            assert_eq!(h.editor_document_text(), "Привет X日本");
            assert_eq!(h.editor_commits, commits);
            assert_eq!(h.editor_redo(), 1);
            assert_eq!(h.editor_document_text(), "Привет 猫日本");
            assert_eq!(h.editor_commits, commits);
            assert_eq!(h.editor_undo(), 1);
            assert_eq!(h.editor_splice(13, 14, "ж"), 1,
                "a new edit must invalidate the redo branch");
            assert_eq!(h.editor_committed, "Привет ж日本");
            assert_eq!(h.editor_redo(), -7);
            assert_eq!(h.editor_focus(false), 0);
            assert_eq!(h.editor_undo(), -1);
            let mut bounded = BrowserHost::new(320, 360);
            assert_eq!(bounded.editor_focus(true), 1);
            for _ in 0..80 {
                let end = bounded.editor_document_text().len();
                assert_eq!(bounded.editor_splice(end, end, "a"), 1);
            }
            assert_eq!(bounded.undo_stack.len(), 64, "journal must be bounded");
        }

        #[test]
        fn r4e2_real_ime_selected_text_commit_cancel_and_blur() {
            let mut h = BrowserHost::new(320, 360);
            let source = "Привет X日本";
            assert_eq!(h.editor_focus(true), 1);
            assert_eq!(h.editor_replace(source), 1);
            assert_eq!(h.editor_commit_selection("猫"), -3,
                "commit must not occur outside active composition");
            assert_eq!(h.editor_begin_composition(13, 14), 1);
            let before_actions = h.editor_actions.get();
            assert_eq!(h.editor_update_preedit("猫"), 1);
            assert!(h.editor_actions.get() > before_actions,
                "selected preedit must reach the real Masonry TextArea");
            assert_eq!(h.editor_committed, source,
                "preedit must not change accepted text");
            assert_eq!(h.editor_live_text(), "Привет 猫日本",
                "selection-aware preview must replace X, not append");
            assert_eq!(h.editor_commit_selection("猫"), 1);
            assert_eq!(h.editor_document_text(), "Привет 猫日本");
            assert_eq!(h.editor_committed, "Привет 猫日本");
            assert_eq!(h.editor_commits, 1);
            assert_eq!(h.editor_commit_selection("猫"), -3,
                "late second commit must not duplicate text");
            assert_eq!(h.editor_commits, 1);
            assert_eq!(h.editor_replace(source), 1);
            assert_eq!(h.editor_begin_composition(13, 14), 1);
            assert_eq!(h.editor_update_preedit("未確定"), 1);
            assert_eq!(h.editor_cancel(), 1);
            assert_eq!(h.editor_document_text(), source,
                "cancel must restore original selected X");
            assert_eq!(h.editor_committed, source);
            assert_eq!(h.editor_replace(source), 1);
            assert_eq!(h.editor_begin_composition(13, 14), 1);
            assert_eq!(h.editor_update_preedit("仮"), 1);
            assert_eq!(h.editor_focus(false), 0);
            assert_eq!(h.editor_document_text(), source,
                "blur must roll back preedit without losing selected text");
            assert_eq!(h.editor_commits, 1);
            assert_eq!(h.editor_focus(true), 1);
            assert_eq!(h.editor_begin_composition(14, 13), -4);
            assert_eq!(h.editor_begin_composition(14, 15), -4,
                "a boundary inside UTF-8 Japanese text must be rejected");
            assert_eq!(h.editor_begin_composition(13, 14), 1);
            assert_eq!(h.editor_commit_selection(&"x".repeat(4097)), -2);
            assert_eq!(h.editor_committed, source);
            assert_eq!(h.editor_cancel(), 1);
            assert_eq!(h.editor_document_text(), source);
            assert_eq!(h.clicks.get(), 0, "IME must never activate scene buttons");
        }

        #[test]
        fn r4e_real_masonry_utf8_selection_splice_and_focus_fence() {
            let mut h = BrowserHost::new(320, 360);
            assert_eq!(h.editor_splice(0, 0, "leak"), -1);
            assert_eq!(h.editor_focus(true), 1);
            assert_eq!(h.editor_replace("A🌍B日本C"), 1);
            let before_actions = h.editor_actions.get();
            assert_eq!(h.editor_select_range(1, 5), 1,
                "select entire four-byte Unicode scalar inside genuine TextArea");
            assert_eq!(h.editor_splice(1, 5, "🐱"), 1);
            assert_eq!(h.editor_document_text(), "A🐱B日本C");
            assert!(h.editor_actions.get() > before_actions,
                "selection splice must emit a genuine upstream TextAction");
            assert_eq!(h.editor_splice(2, 5, "x"), -4,
                "never split a four-byte UTF-8 scalar");
            assert_eq!(h.editor_splice(5, 1, "x"), -4);
            assert_eq!(h.editor_splice(1, 5, ""), 1,
                "deleting a selected emoji must use real TextArea");
            assert_eq!(h.editor_committed, "AB日本C");
            assert_eq!(h.editor_splice(1, 1, "ж"), 1,
                "insert at a collapsed caret in the middle");
            assert_eq!(h.editor_committed, "AжB日本C");
            assert_eq!(h.editor_splice(1, 1, &"z".repeat(4097)), -2,
                "oversized UTF-8 edit must fail closed");
            assert_eq!(h.editor_committed, "AжB日本C");
            // Mirror Chromium's exact Cyrillic/emoji fixture, not just
            // ASCII + CJK. A font/layout regression must fail in native Rust.
            assert_eq!(h.editor_replace("Привет 🌍日本"), 1);
            assert_eq!(h.editor_splice(13, 13, "X"), 1,
                "UTF-8 insertion after Cyrillic text must preserve caret position");
            assert_eq!(h.editor_committed, "Привет X🌍日本");
            assert_eq!(h.editor_splice(14, 18, ""), 1);
            assert_eq!(h.editor_committed, "Привет X日本");
            assert_eq!(h.editor_splice(13, 14, "猫"), 1);
            assert_eq!(h.editor_committed, "Привет 猫日本");
            assert_eq!(h.editor_preedit("に"), 1);
            assert_eq!(h.editor_splice(1, 1, "x"), -3,
                "ordinary input cannot race active composition");
            assert_eq!(h.editor_select_range(1, 1), -3);
            assert_eq!(h.editor_cancel(), 1);
            assert_eq!(h.editor_committed, "Привет 猫日本");
            assert_eq!(h.editor_focus(false), 0);
            assert_eq!(h.editor_splice(1, 1, "leak"), -1);
            assert_eq!(h.clicks.get(), 0,
                "Rust TextArea splice must not activate the scene Button");
        }

        #[test]
        fn r4d_real_masonry_textinput_unicode_preedit_commit_cancel() {
            let mut h = BrowserHost::new(320, 360);
            assert_eq!(h.editor_focused(), 0);
            assert_eq!(h.editor_preedit("leak"), -1,
                "unfocused TextInput must refuse IME");
            assert_eq!(h.editor_focus(true), 1);
            assert_eq!(h.editor_replace("Привет "), 1);
            assert_eq!(h.editor_committed, "Привет ");
            assert_eq!(h.editor_live_text(), "Привет ");
            let clicks = h.clicks.get();
            let commits = h.editor_commits;
            let actions = h.editor_actions.get();
            assert_eq!(h.editor_preedit("に"), 1);
            assert!(h.editor_actions.get() > actions,
                "preedit must reach the actual Masonry TextArea event handler");
            assert_eq!(h.editor_document_text(), "Привет ",
                "upstream PlainEditor must exclude preedit from its source text");
            assert_eq!(h.editor_committed, "Привет ",
                "IME preedit MUST NOT become committed document text");
            assert_ne!(h.editor_live_text(), h.editor_committed,
                "append-only IME preview must remain separate from document text");
            assert_eq!(h.editor_update_preedit("日本"), 1);
            assert_eq!(h.editor_committed, "Привет ");
            assert_eq!(h.editor_replace("wrong"), -3,
                "full-value replacement while composing must fail closed");
            assert_eq!(h.editor_commit("日本"), 1);
            assert_eq!(h.editor_committed, "Привет 日本");
            assert_eq!(h.editor_live_text(), "Привет 日本");
            assert_eq!(h.editor_commits, commits + 1);
            assert_eq!(h.editor_preedit("未確定"), 1);
            assert_eq!(h.editor_cancel(), 1);
            assert_eq!(h.editor_committed, "Привет 日本");
            assert_eq!(h.editor_live_text(), "Привет 日本");
            assert_eq!(h.editor_commit("🌍"), 1);
            assert_eq!(h.editor_committed, "Привет 日本🌍");
            assert_eq!(h.clicks.get(), clicks,
                "text events must not leak to scene ButtonPress");
            assert!(h.editor_actions.get() >= 2,
                "genuine Masonry TextArea must emit TextAction events");
            assert_eq!(h.editor_focus(false), 0);
            assert_eq!(h.editor_commit("leak"), -1,
                "blurred editor must refuse commits");
        }

        #[test]
        fn r4c_real_accesskit_identity_disabled_removal_and_focus() {
            let mut h = BrowserHost::new(320, 360);
            let old_id = h.access_button_id();
            assert_ne!(old_id, 0, "real tagged AccessKit button must have a NodeId");
            assert_eq!(h.access_button_disabled(), 0);
            assert_eq!(h.focus_button(true), 1);
            assert_eq!(h.access_focus_id, old_id, "tree focus must match Rust WidgetId");
            assert_eq!(h.set_button_disabled(true), 1, "disabled bit must reach AccessKit");
            assert_eq!(h.access_button_id(), old_id, "disable must not replace WidgetId");
            assert_eq!(h.focused_button(), 0, "disabled button must lose Rust focus");
            assert_eq!(h.keyboard_button(false, 1), -1, "disabled button cannot activate");
            assert_eq!(h.set_button_disabled(false), 0);
            assert_eq!(h.access_button_id(), old_id, "enable must preserve WidgetId");
            assert_eq!(h.set_button_present(false), 0);
            assert_eq!(h.access_button_id(), 0, "removed node must leave projection");
            assert_eq!(h.access_button_disabled(), -1);
            assert_eq!(h.focus_button(true), -1);
            assert_eq!(h.set_button_present(true), 1);
            let new_id = h.access_button_id();
            assert_ne!(new_id, 0);
            assert_ne!(new_id, old_id, "new Rust widget must get fresh AccessKit NodeId");
            assert_eq!(h.access_button_disabled(), 0);
            assert_eq!(h.focus_button(true), 1);
            assert_eq!(h.access_focus_id, new_id);
            let clicks = h.clicks.get();
            assert_eq!(h.keyboard_button(true, 1), clicks);
            assert_eq!(h.keyboard_button(false, 1), clicks + 1);
            assert_eq!(h.skipped_draws, 0);
        }

        #[test]
        fn real_masonry_focus_and_keyboard_ownership_gate() {
            let mut h = BrowserHost::new(320, 360);
            let start = h.clicks.get();
            assert_eq!(h.semantic_button_id(), 1,
                "the tagged Rust Button must have an AccessKit Button/Click node");
            assert_eq!(h.focused_button(), 0);
            assert_eq!(h.keyboard_button(false, 1), -1,
                "Unfocused scene MUST NOT receive keyboard activation");
            assert_eq!(h.keyboard_events, 0);
            assert_eq!(h.focus_button(true), 1);
            assert_eq!(h.keyboard_button(true, 1), start);
            assert_eq!(h.keyboard_button(false, 1), start + 1,
                "real Masonry Button must emit ButtonPress on Enter key-up");
            assert_eq!(h.keyboard_button(true, 2), start + 1);
            assert_eq!(h.keyboard_button(false, 2), start + 2,
                "real Masonry Button must emit ButtonPress on Space key-up");
            assert_eq!(h.keyboard_button(true, 42), -2, "unknown key must fail closed");
            assert_eq!(h.keyboard_events, 4);
            assert_eq!(h.focus_button(false), 0);
            assert_eq!(h.keyboard_button(false, 1), -1);
            assert_eq!(h.keyboard_events, 4, "blur must fence keyboard events");
            assert_eq!(h.clicks.get(), start + 2);
            assert_eq!(h.skipped_draws, 0);
        }

        #[test]
        fn headless_renderroot_two_widgets_and_resize() {
            let mut h = BrowserHost::new(320, 360);
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
