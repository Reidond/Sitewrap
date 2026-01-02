use std::rc::Rc;

use adw::prelude::*;
use adw::Application;
use anyhow::Result;
use gtk4 as gtk;
use gtk4::glib;
use sitewrap_engine as engine;
use sitewrap_model::{AppPaths, AppRegistry, PermissionRepository, WebAppId};
use tracing::error;

mod manager;
mod permissions_ui;
mod resources;
mod shell;

pub const APP_ID: &str = "xyz.andriishafar.Sitewrap";

#[derive(Clone, Debug)]
pub enum AppMode {
    Manager,
    Shell(WebAppId),
}

#[derive(Clone)]
struct AppContext {
    paths: AppPaths,
    registry: AppRegistry,
    permissions: PermissionRepository,
}

impl AppContext {
    fn new() -> Result<Self> {
        let paths = AppPaths::new()?;
        Ok(Self {
            registry: AppRegistry::new(paths.clone()),
            permissions: PermissionRepository::new(paths.clone()),
            paths,
        })
    }
}

pub fn run(mode: AppMode) -> Result<()> {
    resources::register()?;
    engine::init()?;

    if !sitewrap_portal::is_supported() {
        sitewrap_portal::warn_if_stubbed();
    }

    let app = Application::builder().application_id(APP_ID).build();

    let ctx = Rc::new(AppContext::new()?);
    let mode_for_activate = mode.clone();
    app.connect_activate(move |app| {
        if let Err(err) = on_activate(app, ctx.clone(), mode_for_activate.clone()) {
            error!(target: "app", "failed to activate application: {err:?}");
        }
    });

    glib::timeout_add_local(std::time::Duration::from_millis(16), || {
        engine::tick();
        glib::ControlFlow::Continue
    });

    app.run();
    engine::shutdown();
    Ok(())
}

fn on_activate(app: &Application, ctx: Rc<AppContext>, mode: AppMode) -> Result<()> {
    match mode {
        AppMode::Manager => manager::show(app, ctx),
        AppMode::Shell(id) => shell::show(app, ctx, id),
    }
}

fn builder_from_resource(path: &str) -> gtk::Builder {
    gtk::Builder::from_resource(path)
}
