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
