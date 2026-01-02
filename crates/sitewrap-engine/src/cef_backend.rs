use std::path::PathBuf;

use anyhow::Result;
use gtk4::{prelude::*, Box as GtkBox, Label, Orientation};

use crate::{EngineBackend, EngineConfig};

/// Stub CEF backend placeholder; when the `cef` feature is enabled and libcef bindings are added,
/// this module can be expanded to instantiate a real CEF view. Keeping it separate prevents builds
/// from failing when libcef is not present.

pub struct CefBackend {
    _config: EngineConfig,
}

impl CefBackend {
    pub fn new(config: EngineConfig) -> Result<Self> {
        // When real CEF is wired, validate bindings here (dlopen libcef, etc.).
        Ok(Self { _config: config })
    }

    pub fn available(root: &Option<PathBuf>) -> bool {
        root.as_ref()
            .map(|r| r.join("libcef.so").exists())
            .unwrap_or(false)
    }
}

impl EngineBackend for CefBackend {
    fn build_web_view_with_handler(
        &self,
        start_url: &str,
        _on_navigation: Box<dyn Fn(String) + 'static>,
    ) -> Result<gtk4::Widget> {
        // Placeholder until real CEF bindings are added behind the `cef` feature.
        // TODO (CEF):
        // - dlopen libcef.so from cef_root
        // - init CEF with windowless_rendering_enabled
        // - create OSR browser and wire render handler -> gtk texture
        // - forward input events from GTK controllers
        // - connect navigation callbacks to `_on_navigation`
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        let label = Label::new(Some(
            "CEF backend placeholder (feature enabled, bindings not yet wired)",
        ));
        label.set_xalign(0.0);
        container.append(&label);

        Ok(container.upcast())
    }

    fn tick_hook(&self) -> Option<Box<dyn Fn() + Send + Sync>> {
        None
    }
}
