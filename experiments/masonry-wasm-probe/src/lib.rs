//! Compile-only probe, NOT a browser host or a widget interaction test.
//! Xilem pin matches Akula, but no Akula source is used.
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
pub fn headless_two_widget_scene_count() -> usize {
    use masonry::app::{RenderRoot, RenderRootOptions, WindowSizePolicy};
    use masonry::core::{NewWidget, PropertySet, Widget, WindowEvent};
    use masonry::dpi::PhysicalSize;
    use masonry::layout::Length;
    use masonry::peniko::Color;
    use masonry::properties::Background;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Flex, SizedBox};
    use std::sync::Arc;

    // No fonts or native windowing: only two colored widgets and a layout tree.
    let tile = |r, g, b| {
        NewWidget::new(
            SizedBox::empty().size(Length::const_px(120.0), Length::const_px(50.0))
        ).with_props(PropertySet::one(Background::Color(Color::from_rgb8(r, g, b))))
    };
    let root = Flex::column()
        .with_fixed(tile(70, 120, 210))
        .with_fixed(tile(195, 145, 105))
        .prepare();

    let mut runtime = RenderRoot::new(
        root,
        |_| {},
        RenderRootOptions {
            default_properties: Arc::new(default_property_set()),
            use_system_fonts: false,
            size_policy: WindowSizePolicy::User,
            size: PhysicalSize::new(320, 240),
            scale_factor: 1.0,
            test_font: None,
        },
    );
    let (first, _) = runtime.redraw();
    assert!(first.root_layer().is_some(), "Masonry RenderRoot must produce a scene");
    runtime.handle_window_event(WindowEvent::Resize(PhysicalSize::new(400, 300)));
    let (second, _) = runtime.redraw();
    assert!(second.root_layer().is_some(), "Resized RenderRoot must produce a scene");
    second.layers.len()
}

#[cfg(all(test, feature = "widgets", not(target_arch = "wasm32")))]
#[test]
fn native_headless_renderroot_scene_and_resize() {
    assert!(headless_two_widget_scene_count() > 0);
}
