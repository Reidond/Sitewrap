use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use gtk4::{prelude::*, Box as GtkBox, Button, Label, Orientation};
use tracing::info;

#[cfg(feature = "cef")]
mod cef_backend;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub profile_dir: PathBuf,
    /// Optional root directory containing CEF binaries/assets (libcef.so, locales, pak files).
    pub cef_root: Option<PathBuf>,
}

impl EngineConfig {
    pub fn new(profile_dir: PathBuf) -> Self {
        Self {
            profile_dir,
            cef_root: std::env::var_os("SITEWRAP_CEF_ROOT")
                .or_else(|| std::env::var_os("CEF_ROOT"))
                .map(PathBuf::from),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineMode {
    Stub,
    CefUnavailable,
    CefReady,
}

pub struct Engine {
    backend: Box<dyn EngineBackend>,
}

#[allow(clippy::type_complexity)]
static TICK_HOOK: OnceLock<Mutex<Option<Box<dyn Fn() + Send + Sync>>>> = OnceLock::new();

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self> {
        let mode = detect_cef(&config)?;
        info!(
            target: "engine",
            profile = ?config.profile_dir,
            mode = %describe_mode(mode),
            cef_root = ?config.cef_root,
            "initializing engine"
        );

        let backend: Box<dyn EngineBackend> = match mode {
            #[cfg(feature = "cef")]
            EngineMode::CefReady if cef_backend::CefBackend::available(&config.cef_root) => {
                Box::new(cef_backend::CefBackend::new(config)?)
            }
            EngineMode::CefReady => Box::new(PlaceholderCefBackend { config }),
            _ => Box::new(StubBackend { config }),
        };
        let hook_slot = TICK_HOOK.get_or_init(|| Mutex::new(None));
        *hook_slot.lock().unwrap() = backend.tick_hook();

        Ok(Self { backend })
    }

    pub fn build_web_view(&self, start_url: &str) -> Result<gtk4::Widget> {
        self.build_web_view_with_handler(start_url, |_| {}, |_| {})
    }

    pub fn build_web_view_with_handler<F>(
        &self,
        start_url: &str,
        on_navigation: F,
        on_permission: impl Fn(PermissionKind) + 'static,
    ) -> Result<gtk4::Widget>
    where
        F: Fn(String) + 'static,
    {
        self.backend.build_web_view_with_handler(
            start_url,
            Box::new(on_navigation),
            Box::new(on_permission),
        )
    }
}

fn detect_cef(config: &EngineConfig) -> Result<EngineMode> {
    if let Some(root) = &config.cef_root {
        let libcef = root.join("libcef.so");
        if libcef.exists() {
            return Ok(EngineMode::CefReady);
        }
        return Ok(EngineMode::CefUnavailable);
    }
    Ok(EngineMode::Stub)
}

fn describe_mode(mode: EngineMode) -> &'static str {
    match mode {
        EngineMode::Stub => "stub",
        EngineMode::CefUnavailable => "cef-missing",
        EngineMode::CefReady => "cef-ready",
    }
}

pub fn init() -> Result<()> {
    // In real implementation, wire global CEF init and subprocess args.
    info!(target: "engine", "stub init");
    Ok(())
}

/// Pump the engine message loop; placeholder until CEF is wired.
pub fn tick() {
    if let Some(slot) = TICK_HOOK.get() {
        if let Some(cb) = slot.lock().unwrap().as_ref() {
            (cb)();
        }
    }
}

pub fn shutdown() {
    info!(target: "engine", "stub shutdown");
}

trait EngineBackend: 'static {
    fn build_web_view_with_handler(
        &self,
        start_url: &str,
        on_navigation: Box<dyn Fn(String) + 'static>,
        on_permission: Box<dyn Fn(PermissionKind) + 'static>,
    ) -> Result<gtk4::Widget>;

    /// Optional per-backend message loop tick hook; called every ~16ms from the main loop.
    fn tick_hook(&self) -> Option<Box<dyn Fn() + Send + Sync>> {
        None
    }
}

struct StubBackend {
    #[allow(dead_code)] // Will be used when CEF is wired
    config: EngineConfig,
}

impl EngineBackend for StubBackend {
    fn build_web_view_with_handler(
        &self,
        start_url: &str,
        on_navigation: Box<dyn Fn(String) + 'static>,
        _on_permission: Box<dyn Fn(PermissionKind) + 'static>,
    ) -> Result<gtk4::Widget> {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        let label = Label::new(Some("CEF view placeholder\nNavigation hooks are stubbed"));
        label.set_xalign(0.0);
        container.append(&label);

        let on_navigation = Rc::new(on_navigation);

        let internal_btn = Button::with_label("Navigate (same origin)");
        let start = start_url.to_string();
        let nav_clone = on_navigation.clone();
        internal_btn.connect_clicked(move |_| {
            nav_clone(start.clone());
        });

        let external_btn = Button::with_label("Navigate external example.org");
        external_btn.connect_clicked(move |_| {
            on_navigation("https://example.org".to_string());
        });

        container.append(&internal_btn);
        container.append(&external_btn);

        Ok(container.upcast())
    }
}

/// Placeholder backend selected when CEF assets are detected but no compiled bindings are present.
/// Keeps runtime behavior consistent while signaling that CEF is available once wired.
struct PlaceholderCefBackend {
    #[allow(dead_code)] // Will be used when CEF is wired
    config: EngineConfig,
}

impl EngineBackend for PlaceholderCefBackend {
    fn build_web_view_with_handler(
        &self,
        start_url: &str,
        on_navigation: Box<dyn Fn(String) + 'static>,
        _on_permission: Box<dyn Fn(PermissionKind) + 'static>,
    ) -> Result<gtk4::Widget> {
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        let label = Label::new(Some(
            "CEF assets detected; rendering stub until CEF backend is wired",
        ));
        label.set_xalign(0.0);
        container.append(&label);

        let on_navigation = Rc::new(on_navigation);

        let internal_btn = Button::with_label("Navigate (same origin)");
        let start = start_url.to_string();
        let nav_clone = on_navigation.clone();
        internal_btn.connect_clicked(move |_| {
            nav_clone(start.clone());
        });

        let external_btn = Button::with_label("Navigate external example.org");
        external_btn.connect_clicked(move |_| {
            on_navigation("https://example.org".to_string());
        });

        container.append(&internal_btn);
        container.append(&external_btn);

        Ok(container.upcast())
    }
}
#[derive(Clone, Copy, Debug)]
pub enum PermissionKind {
    Notifications,
    Camera,
    Microphone,
    Location,
}
