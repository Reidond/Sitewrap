use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use adw::prelude::MessageDialogExtManual;
use adw::prelude::*;
use anyhow::{anyhow, Context, Result};
use gdk4 as gdk;
use gio::prelude::*;
use gtk4 as gtk;
use gtk4::prelude::GtkDialogExtManual;
use sitewrap_engine::{Engine, EngineConfig};
use sitewrap_model::{
    PerOriginPermissions, PermissionState, PermissionStore, WebAppDefinition, WebAppId,
};
use sitewrap_portal::{self, NotificationRequest, SaveFileRequest};

use crate::{builder_from_resource, permissions_ui::*, AppContext};

const SHELL_UI: &str = "/xyz/andriishafar/sitewrap/ui/shell_window.ui";

struct ShellState {
    ctx: Rc<AppContext>,
    app_def: WebAppDefinition,
    window: adw::ApplicationWindow,
    content: gtk::Box,
    nav_bar: gtk::Box,
    toast_overlay: adw::ToastOverlay,
    engine: RefCell<Rc<Engine>>,
    current_url: RefCell<String>,
    view: RefCell<gtk::Widget>,
}

pub fn show(app: &adw::Application, ctx: Rc<AppContext>, app_id: WebAppId) -> Result<()> {
    let mut app_def = ctx
        .registry
        .load(app_id)
        .with_context(|| format!("load web app {app_id}"))?;

    let builder = builder_from_resource(SHELL_UI)?;
    let window: adw::ApplicationWindow = builder
        .object("shell_window")
        .context("shell_window missing in blueprint")?;
    let content: gtk::Box = builder
        .object("shell_content")
        .context("shell_content missing in blueprint")?;
    let nav_bar: gtk::Box = builder
        .object("shell_nav_bar")
        .context("shell_nav_bar missing in blueprint")?;
    let menu_button: gtk::MenuButton = builder
        .object("shell_menu_button")
        .context("shell_menu_button missing in blueprint")?;
    let title: adw::WindowTitle = builder
        .object("shell_title")
        .context("shell_title missing in blueprint")?;
    let toast_overlay: adw::ToastOverlay = builder
        .object("shell_toast_overlay")
        .context("shell_toast_overlay missing in blueprint")?;

    window.set_title(Some(&app_def.name));
    window.set_application(Some(app));
    title.set_title(&app_def.name);
    title.set_subtitle(Some(&app_def.primary_origin));

    // Update last launched and persist so manager reflects launches from shell.
    app_def.last_launched_at = Some(time::OffsetDateTime::now_utc());
    ctx.registry.save(&app_def)?;

    let engine = Rc::new(Engine::new(EngineConfig::new(
        ctx.paths.profile_dir(app_id),
    ))?);
    let engine_for_nav = Rc::clone(&engine);
    let state_placeholder = Rc::new(RefCell::new(None::<Rc<ShellState>>));
    let view = {
        let state_placeholder = Rc::clone(&state_placeholder);
        engine_for_nav.build_web_view_with_handler(&app_def.start_url, move |target| {
            if let Some(state) = state_placeholder.borrow().as_ref() {
                if let Err(err) = handle_navigation_request(state, &target) {
                    tracing::error!(target: "ui", "navigation handler failed: {err:?}");
                }
            }
        })?
    };
    view.set_hexpand(true);
    view.set_vexpand(true);
    content.append(&view);

    let current_url = view_url(&app_def);
    let state = Rc::new(ShellState {
        ctx,
        app_def,
        window: window.clone(),
        content: content.clone(),
        nav_bar: nav_bar.clone(),
        toast_overlay,
        engine: RefCell::new(engine),
        current_url: RefCell::new(current_url),
        view: RefCell::new(view),
    });
    state_placeholder.replace(Some(Rc::clone(&state)));

    setup_menu(&state, &menu_button);
    setup_nav_bar(&state);

    window.present();
    Ok(())
}

fn view_url(app_def: &WebAppDefinition) -> String {
    app_def.start_url.clone()
}

fn is_external_navigation(app_def: &WebAppDefinition, target: &str) -> bool {
    if !app_def.behavior.open_external_links {
        return false;
    }
    if let Ok(url) = url::Url::parse(target) {
        let origin = url.origin().ascii_serialization();
        origin != app_def.primary_origin
    } else {
        false
    }
}

fn setup_menu(state: &Rc<ShellState>, menu_button: &gtk::MenuButton) {
    let menu = build_shell_menu();
    menu_button.set_menu_model(Some(&menu));

    let reload_action = gio::SimpleAction::new("reload", None);
    let state_reload = Rc::clone(state);
    reload_action.connect_activate(move |_, _| {
        if let Err(err) = reload_view(&state_reload) {
            tracing::error!(target: "ui", "reload failed: {err:?}");
            show_error_dialog(&state_reload, "Reload failed", &err);
        }
    });
    state.window.add_action(&reload_action);

    let copy_action = gio::SimpleAction::new("copy_link", None);
    let state_copy = Rc::clone(state);
    copy_action.connect_activate(move |_, _| {
        if let Err(err) = copy_link(&state_copy) {
            tracing::error!(target: "ui", "copy link failed: {err:?}");
            show_error_dialog(&state_copy, "Copy link failed", &err);
        }
    });
    state.window.add_action(&copy_action);

    let open_action = gio::SimpleAction::new("open_in_browser", None);
    let state_open = Rc::clone(state);
    open_action.connect_activate(move |_, _| {
        if let Err(err) = open_in_browser(&state_open) {
            tracing::error!(target: "ui", "open in browser failed: {err:?}");
            show_error_dialog(&state_open, "Open in default browser failed", &err);
        }
    });
    state.window.add_action(&open_action);

    let permissions_action = gio::SimpleAction::new("permissions", None);
    let state_permissions = Rc::clone(state);
    permissions_action.connect_activate(move |_, _| {
        if let Err(err) = open_permissions_window(&state_permissions) {
            tracing::error!(target: "ui", "open permissions failed: {err:?}");
            show_error_dialog(&state_permissions, "Permissions unavailable", &err);
        }
    });
    state.window.add_action(&permissions_action);

    let clear_action = gio::SimpleAction::new("clear_data", None);
    let state_clear = Rc::clone(state);
    clear_action.connect_activate(move |_, _| {
        confirm_clear_data(&state_clear);
    });
    state.window.add_action(&clear_action);

    let save_action = gio::SimpleAction::new("save_dummy", None);
    let state_save = Rc::clone(state);
    save_action.connect_activate(move |_, _| {
        if let Err(err) = trigger_dummy_save(&state_save) {
            tracing::error!(target: "ui", "save failed: {err:?}");
            show_error_dialog(&state_save, "Save failed", &err);
        }
    });
    state.window.add_action(&save_action);

    let about_action = gio::SimpleAction::new("about", None);
    let state_about = Rc::clone(state);
    about_action.connect_activate(move |_, _| {
        open_about_window(&state_about);
    });
    state.window.add_action(&about_action);

    let notify_action = gio::SimpleAction::new("test_notification", None);
    let state_notify = Rc::clone(state);
    notify_action.connect_activate(move |_, _| {
        if let Err(err) = trigger_notification(&state_notify) {
            tracing::error!(target: "ui", "notification test failed: {err:?}");
            show_error_dialog(&state_notify, "Notification failed", &err);
        }
    });
    state.window.add_action(&notify_action);

    // Gracefully handle missing portals.
    let portal_ok = sitewrap_portal::is_supported();
    let file_portal_ok = sitewrap_portal::is_file_chooser_supported();
    let open_uri_ok = sitewrap_portal::is_open_uri_supported();
    if !portal_ok {
        permissions_action.set_enabled(false);
        notify_action.set_enabled(false);
        show_toast(state, "Desktop portals unavailable; some actions disabled");
    }
    if !open_uri_ok {
        open_action.set_enabled(false);
    }
    if !file_portal_ok {
        save_action.set_enabled(false);
    }
}

fn build_shell_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let navigation = gio::Menu::new();
    navigation.append(Some("Reload"), Some("win.reload"));
    navigation.append(Some("Copy Link"), Some("win.copy_link"));
    navigation.append(Some("Open in Default Browser"), Some("win.open_in_browser"));
    menu.append_section(None, &navigation);

    let settings = gio::Menu::new();
    settings.append(Some("Permissions"), Some("win.permissions"));
    settings.append(Some("Clear Data"), Some("win.clear_data"));
    settings.append(Some("Save Page As… (placeholder)"), Some("win.save_dummy"));
    settings.append(
        Some("Test Notification (placeholder)"),
        Some("win.test_notification"),
    );
    menu.append_section(None, &settings);

    let about = gio::Menu::new();
    about.append(Some("About"), Some("win.about"));
    menu.append_section(None, &about);

    menu
}

fn setup_nav_bar(state: &Rc<ShellState>) {
    // Clear any existing children to avoid duplicates on rebuilds.
    for child in state.nav_bar.children() {
        state.nav_bar.remove(&child);
    }

    state
        .nav_bar
        .set_visible(state.app_def.behavior.show_navigation);

    if !state.app_def.behavior.show_navigation {
        return;
    }

    let back_btn = gtk::Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back (requires CEF engine)")
        .sensitive(false)
        .build();

    let forward_btn = gtk::Button::builder()
        .icon_name("go-next-symbolic")
        .tooltip_text("Forward (requires CEF engine)")
        .sensitive(false)
        .build();

    let reload_btn = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Reload")
        .build();

    let state_reload = Rc::clone(state);
    reload_btn.connect_clicked(move |_| {
        if let Err(err) = reload_view(&state_reload) {
            tracing::error!(target: "ui", "reload failed: {err:?}");
            show_error_dialog(&state_reload, "Reload failed", &err);
        }
    });

    state.nav_bar.append(&back_btn);
    state.nav_bar.append(&forward_btn);
    state.nav_bar.append(&reload_btn);
}

fn reload_view(state: &ShellState) -> Result<()> {
    let url = state.current_url.borrow().clone();
    let engine = state.engine.borrow().clone();
    let view = engine.build_web_view(&url)?;
    view.set_hexpand(true);
    view.set_vexpand(true);
    for child in state.content.children() {
        state.content.remove(&child);
    }
    state.content.append(&view);
    state.content.show();
    state.view.replace(view);
    show_toast(state, "Reloaded");
    Ok(())
}

fn copy_link(state: &ShellState) -> Result<()> {
    let url = state.current_url.borrow().clone();
    let display = gdk::Display::default().ok_or_else(|| anyhow!("no display"))?;
    display.clipboard().set_text(&url);
    show_toast(state, "Link copied");
    Ok(())
}

fn open_in_browser(state: &ShellState) -> Result<()> {
    let url = state.current_url.borrow().clone();
    sitewrap_portal::open_uri(&url).context("open uri")?;
    Ok(())
}

fn handle_navigation_request(state: &ShellState, target: &str) -> Result<()> {
    if is_external_navigation(&state.app_def, target) {
        sitewrap_portal::open_uri(target).context("open external via portal")?;
        show_toast(state, "Opened externally");
    } else {
        // With a real engine we would load in place; stub just records.
        state.current_url.replace(target.to_string());
        show_toast(state, "Navigated");
    }
    Ok(())
}

fn trigger_notification(state: &Rc<ShellState>) -> Result<()> {
    let origin = state.app_def.primary_origin.clone();
    let mut store = state
        .ctx
        .permissions
        .load(state.app_def.id)
        .context("load permissions")?;
    let entry = store.get_or_default_mut(&origin);

    match entry.notifications {
        PermissionState::Allow => {
            show_toast(state, "Notifications allowed (sending)");
            send_sample_notification(state, &origin)?;
        }
        PermissionState::Block => {
            show_toast(state, "Notifications blocked (change in Permissions)");
        }
        PermissionState::Ask => {
            let state_clone = Rc::clone(state);
            let origin_clone = origin.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Err(err) = handle_notification_prompt_async(state_clone, origin_clone).await
                {
                    tracing::error!(target: "ui", "notification prompt failed: {err:?}");
                }
            });
        }
    }
    Ok(())
}

fn trigger_dummy_save(state: &ShellState) -> Result<()> {
    let request = sitewrap_portal::SaveFileRequest {
        title: format!("Save page - {}", state.app_def.name),
        suggested_name: format!("{}-page.txt", state.app_def.name.replace(' ', "_")),
        default_directory: None,
        content: format!(
            "Dummy export for {}\nURL: {}\n",
            state.app_def.name,
            state.current_url.borrow()
        )
        .into_bytes(),
    };
    match sitewrap_portal::save_file(&request) {
        Ok(()) => show_toast(state, "Saved placeholder export"),
        Err(err) => show_error_dialog(state, "Save failed", &err),
    }
    Ok(())
}

async fn handle_notification_prompt_async(state: Rc<ShellState>, origin: String) -> Result<()> {
    let decision = prompt_notification_permission_async(&state, &origin).await?;

    let mut store = state
        .ctx
        .permissions
        .load(state.app_def.id)
        .context("load permissions")?;
    let entry = store.get_or_default_mut(&origin);
    entry.notifications = decision;
    state
        .ctx
        .permissions
        .save(state.app_def.id, &store)
        .context("save permissions")?;

    match decision {
        PermissionState::Allow => {
            show_toast(&state, "Notifications allowed (sending)");
            send_sample_notification(&state, &origin)?;
        }
        PermissionState::Block => {
            show_toast(&state, "Notifications blocked (change in Permissions)");
        }
        PermissionState::Ask => {
            show_toast(&state, "Notification dismissed");
        }
    }
    Ok(())
}

async fn prompt_notification_permission_async(
    state: &Rc<ShellState>,
    origin: &str,
) -> Result<PermissionState> {
    let dialog = adw::MessageDialog::builder()
        .transient_for(&state.window)
        .heading(format!("Allow notifications for {}?", origin))
        .body("This site wants to show notifications.")
        .build();
    dialog.add_response("block", "Block");
    dialog.add_response("allow", "Allow");
    dialog.set_default_response(Some("allow"));
    dialog.set_close_response("block");

    let response = dialog.run_future().await;
    dialog.close();
    let decision = if response.as_str() == "allow" {
        PermissionState::Allow
    } else {
        PermissionState::Block
    };
    Ok(decision)
}

fn send_sample_notification(state: &ShellState, origin: &str) -> Result<()> {
    let request = NotificationRequest {
        app_id: state.app_def.icon_id.clone(),
        title: format!("{} says hi", state.app_def.name),
        body: format!("Sample notification for {}", origin),
        icon: Some(state.app_def.icon_id.clone()),
    };
    sitewrap_portal::send_notification(&request).context("send notification")?;
    show_toast(state, "Notification sent (placeholder)");
    Ok(())
}

fn open_permissions_window(state: &Rc<ShellState>) -> Result<()> {
    let mut store = state
        .ctx
        .permissions
        .load(state.app_def.id)
        .context("load permissions")?;

    // Ensure the primary origin is always present.
    store.get_or_default_mut(&state.app_def.primary_origin);

    // Stable order for UI.
    let mut origins: BTreeMap<String, PermissionSnapshot> = BTreeMap::new();
    for (origin, entry) in store.origins.iter() {
        origins.insert(
            origin.clone(),
            PermissionSnapshot {
                notifications: entry.notifications.clone(),
                camera: entry.camera.clone(),
                microphone: entry.microphone.clone(),
                location: entry.location.clone(),
            },
        );
    }

    let window = adw::PreferencesWindow::builder()
        .transient_for(&state.window)
        .modal(true)
        .title(format!("Permissions - {}", state.app_def.name))
        .default_width(520)
        .default_height(480)
        .build();

    let page = adw::PreferencesPage::builder().title("Permissions").build();
    let store = Rc::new(RefCell::new(store));
    let repo = state.ctx.permissions.clone();
    let app_id = state.app_def.id;

    fn rebuild_page(
        page: &adw::PreferencesPage,
        store: &Rc<RefCell<PermissionStore>>,
        repo: &sitewrap_model::PermissionRepository,
        app_id: WebAppId,
    ) {
        for child in page.children() {
            page.remove(&child);
        }

        let mut origins: BTreeMap<String, PermissionSnapshot> = BTreeMap::new();
        for (origin, entry) in store.borrow().origins.iter() {
            origins.insert(
                origin.clone(),
                PermissionSnapshot {
                    notifications: entry.notifications.clone(),
                    camera: entry.camera.clone(),
                    microphone: entry.microphone.clone(),
                    location: entry.location.clone(),
                },
            );
        }

        for (origin, snapshot) in origins.into_iter() {
            let group = adw::PreferencesGroup::builder()
                .title(origin.as_str())
                .build();

            add_permission_row(
                &group,
                "Notifications",
                snapshot.notifications,
                PermissionField::Notifications,
                Rc::clone(store),
                origin.clone(),
                repo.clone(),
                app_id,
            );
            add_permission_row(
                &group,
                "Camera",
                snapshot.camera,
                PermissionField::Camera,
                Rc::clone(store),
                origin.clone(),
                repo.clone(),
                app_id,
            );
            add_permission_row(
                &group,
                "Microphone",
                snapshot.microphone,
                PermissionField::Microphone,
                Rc::clone(store),
                origin.clone(),
                repo.clone(),
                app_id,
            );
            add_permission_row(
                &group,
                "Location",
                snapshot.location,
                PermissionField::Location,
                Rc::clone(store),
                origin,
                repo.clone(),
                app_id,
            );

            page.add(&group);
        }

        let page_clone = page.clone();
        let store_clone = Rc::clone(store);
        let repo_clone = repo.clone();
        add_origin_row(&page, move |origin| {
            store_clone.borrow_mut().get_or_default_mut(origin);
            if let Err(err) = repo_clone.save(app_id, &store_clone.borrow()) {
                tracing::error!(target: "ui", "save permissions failed: {err:?}");
            }
            rebuild_page(&page_clone, &store_clone, &repo_clone, app_id);
        });
    }

    rebuild_page(&page, &store, &repo, app_id);

    window.add(&page);
    window.present();
    Ok(())
}

fn confirm_clear_data(state: &Rc<ShellState>) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(&state.window)
        .heading(format!("Clear data for {}?", state.app_def.name))
        .body("This will clear cookies, storage, cache, and permissions for this web app.")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("clear", "Clear");
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let state_clear = Rc::clone(state);
    dialog.connect_response(move |d, resp| {
        if resp == "clear" {
            if let Err(err) = run_clear_data(&state_clear) {
                tracing::error!(target: "ui", "clear data failed: {err:?}");
                show_error_dialog(&state_clear, "Clear data failed", &err);
            }
        }
        d.close();
    });
    dialog.present();
}

fn run_clear_data(state: &ShellState) -> Result<()> {
    state
        .ctx
        .permissions
        .delete(state.app_def.id)
        .context("delete permissions")?;
    state
        .ctx
        .paths
        .delete_profile_dir(state.app_def.id)
        .context("delete profile dir")?;
    state
        .ctx
        .paths
        .delete_icons_for(&state.app_def.icon_id)
        .context("delete icons")?;

    // Recreate engine/profile so subsequent loads use a clean profile.
    let profile_dir = state.ctx.paths.profile_dir(state.app_def.id);
    std::fs::create_dir_all(&profile_dir).context("create profile dir")?;
    let new_engine = Rc::new(Engine::new(EngineConfig::new(profile_dir))?);
    let url = view_url(&state.app_def);
    let view = new_engine.build_web_view(&url)?;
    view.set_hexpand(true);
    view.set_vexpand(true);
    for child in state.content.children() {
        state.content.remove(&child);
    }
    state.content.append(&view);
    state.content.show();
    state.view.replace(view);
    state.engine.replace(new_engine);
    state.current_url.replace(url);
    show_toast(state, "Data cleared");
    Ok(())
}

fn open_about_window(state: &ShellState) {
    let about = adw::AboutWindow::builder()
        .transient_for(&state.window)
        .application_name(&state.app_def.name)
        .application_icon(&state.app_def.icon_id)
        .developer_name("Sitewrap")
        .version(env!("CARGO_PKG_VERSION"))
        .website(&state.app_def.start_url)
        .build();
    about.present();
}

fn show_toast(state: &ShellState, message: &str) {
    let toast = adw::Toast::new(message);
    state.toast_overlay.add_toast(toast);
}

fn show_error_dialog(state: &ShellState, heading: &str, err: &anyhow::Error) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(&state.window)
        .heading(heading)
        .body(err.to_string())
        .build();
    dialog.add_response("close", "OK");
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");
    dialog.connect_response(|d, _| d.close());
    dialog.present();
}

struct PermissionSnapshot {
    notifications: PermissionState,
    camera: PermissionState,
    microphone: PermissionState,
    location: PermissionState,
}
