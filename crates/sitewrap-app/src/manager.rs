use std::{cell::RefCell, rc::Rc, thread};

use adw::prelude::*;
use anyhow::{bail, Context, Result};
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::glib;
use sitewrap_icons::fetch_and_cache_icon;
use sitewrap_model::{
    normalize_url, AppPaths, PerOriginPermissions, PermissionState, WebAppDefinition, WebAppId,
};
use sitewrap_portal::{install_launcher, remove_launcher, warn_if_stubbed, LauncherDescriptor};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use url::Url;

use crate::{builder_from_resource, permissions_ui::*, AppContext};

const MANAGER_UI: &str = "/xyz/andriishafar/sitewrap/ui/manager_window.ui";

#[derive(Clone)]
struct Handlers {
    ctx: Rc<AppContext>,
    apps: Rc<RefCell<Vec<WebAppDefinition>>>,
    list: gtk::ListBox,
    search_entry: gtk::SearchEntry,
    window: adw::ApplicationWindow,
}

fn desktop_id_for(app: &WebAppDefinition) -> String {
    format!("{}.desktop", app.icon_id)
}

fn launcher_descriptor_for(app: &WebAppDefinition, paths: &AppPaths) -> LauncherDescriptor {
    let icon_path = paths
        .icons_cache_dir()
        .join(format!("{}-128x128.png", app.icon_id));
    LauncherDescriptor {
        desktop_id: desktop_id_for(app),
        name: app.name.clone(),
        exec: format!("sitewrap --shell {}", app.id),
        icon_name: app.icon_id.clone(),
        icon_file: icon_path.exists().then_some(icon_path),
    }
}

pub fn show(app: &adw::Application, ctx: Rc<AppContext>) -> Result<()> {
    let builder = builder_from_resource(MANAGER_UI)?;
    let window: adw::ApplicationWindow = builder
        .object("manager_window")
        .context("manager_window missing in blueprint")?;
    let list: gtk::ListBox = builder
        .object("apps_list")
        .context("apps_list missing in blueprint")?;
    let create_btn: gtk::Button = builder
        .object("create_button")
        .context("create_button missing in blueprint")?;
    let search_entry: gtk::SearchEntry = builder
        .object("search_entry")
        .context("search_entry missing in blueprint")?;

    window.set_application(Some(app));

    let apps = Rc::new(RefCell::new(ctx.registry.list()?));

    if !sitewrap_portal::is_supported() {
        let dialog = adw::MessageDialog::builder()
            .transient_for(&window)
            .heading("Desktop integration unavailable")
            .body("xdg-desktop-portal is not available; launchers and notifications will be disabled.")
            .build();
        dialog.add_response("close", "OK");
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.connect_response(|d, _| d.close());
        dialog.present();
    } else {
        warn_if_stubbed();
    }

    let handlers = Handlers {
        ctx: Rc::clone(&ctx),
        apps: Rc::clone(&apps),
        list: list.clone(),
        search_entry: search_entry.clone(),
        window: window.clone(),
    };

    let refresh_list = {
        let list = list.clone();
        let apps = Rc::clone(&apps);
        let handlers = handlers.clone();
        move |query: &str| refresh_listbox(&list, &apps.borrow(), query, &handlers)
    };

    refresh_list("");

    {
        let window = window.clone();
        let handlers = handlers.clone();
        create_btn.connect_clicked(glib::clone!(@weak app => move |_| {
            if let Err(err) = open_create_window(&app, &window, handlers.clone()) {
                tracing::error!(target: "ui", "failed to open create window: {err:?}");
            }
        }));
    }

    {
        let refresh = refresh_list.clone();
        search_entry.connect_search_changed(move |entry| refresh(&entry.text()));
    }

    window.present();
    Ok(())
}

fn refresh_listbox(
    list: &gtk::ListBox,
    apps: &[WebAppDefinition],
    query: &str,
    handlers: &Handlers,
) {
    for row in list.children() {
        list.remove(&row);
    }
    let needle = query.trim().to_ascii_lowercase();
    let mut sorted = apps.to_vec();
    sorted.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    for app in sorted.iter() {
        if !needle.is_empty()
            && !app.name.to_ascii_lowercase().contains(&needle)
            && !app.start_url.to_ascii_lowercase().contains(&needle)
            && !app.primary_origin.to_ascii_lowercase().contains(&needle)
            && !(app.behavior.open_external_links && "external".contains(&needle))
            && !(app.behavior.show_navigation && "navigation".contains(&needle))
        {
            continue;
        }
        let row = make_row(app, handlers);
        list.append(&row);
    }
    list.show();
}

fn make_row(app: &WebAppDefinition, handlers: &Handlers) -> gtk::Widget {
    let row = gtk::ListBoxRow::builder()
        .selectable(false)
        .activatable(false)
        .build();
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();

    let icon = icon_widget(app, &handlers.ctx.paths);

    let r#box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();
    let title = gtk::Label::builder()
        .label(&app.name)
        .xalign(0.0)
        .css_classes(vec!["title-3".into()])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let subtitle = gtk::Label::builder()
        .label(&app.start_url)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .wrap(true)
        .css_classes(vec!["dim-label".into()])
        .build();
    let behavior_label = gtk::Label::builder()
        .label(&format!(
            "External: {} • Nav: {}",
            if app.behavior.open_external_links {
                "On"
            } else {
                "Off"
            },
            if app.behavior.show_navigation {
                "On"
            } else {
                "Off"
            }
        ))
        .xalign(0.0)
        .css_classes(vec!["dim-label".into()])
        .build();
    let last_launched = gtk::Label::builder()
        .label(&format!("Last launched: {}", format_last_launched(app)))
        .xalign(0.0)
        .css_classes(vec!["dim-label".into()])
        .build();
    r#box.append(&title);
    r#box.append(&subtitle);
    r#box.append(&behavior_label);
    r#box.append(&last_launched);

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::Start)
        .build();
    let launch_btn = gtk::Button::with_label("Launch");
    launch_btn.add_css_class("suggested-action");
    let edit_btn = gtk::Button::with_label("Edit");
    edit_btn.add_css_class("flat");
    let permissions_btn = gtk::Button::with_label("Permissions");
    permissions_btn.add_css_class("flat");
    let reset_btn = gtk::Button::with_label("Reset data");
    reset_btn.add_css_class("flat");
    let remove_btn = gtk::Button::with_label("Remove");
    remove_btn.add_css_class("destructive-action");
    actions.append(&launch_btn);
    actions.append(&edit_btn);
    actions.append(&permissions_btn);
    actions.append(&reset_btn);
    actions.append(&remove_btn);
    r#box.append(&actions);

    root.append(&icon);
    root.append(&r#box);
    row.set_child(Some(&root));

    let app_launch = app.clone();
    let handlers_launch = handlers.clone();
    launch_btn.connect_clicked(move |_| {
        if let Err(err) = run_launch(&handlers_launch, &app_launch) {
            tracing::error!(target: "ui", "launch failed: {err:?}");
        }
    });

    let app_edit = app.clone();
    let handlers_edit = handlers.clone();
    edit_btn.connect_clicked(move |_| {
        if let Err(err) = open_edit_window(&handlers_edit, &app_edit) {
            tracing::error!(target: "ui", "edit failed: {err:?}");
        }
    });

    let app_permissions = app.clone();
    let handlers_permissions = handlers.clone();
    permissions_btn.connect_clicked(move |_| {
        if let Err(err) = open_permissions_window_manager(&handlers_permissions, &app_permissions) {
            tracing::error!(target: "ui", "permissions failed: {err:?}");
        }
    });

    let app_reset = app.clone();
    let handlers_reset = handlers.clone();
    reset_btn.connect_clicked(move |_| {
        confirm_reset(&handlers_reset, &app_reset);
    });

    let app_remove = app.clone();
    let handlers_remove = handlers.clone();
    remove_btn.connect_clicked(move |_| {
        confirm_remove(&handlers_remove, &app_remove);
    });

    row.upcast()
}

fn icon_widget(app: &WebAppDefinition, paths: &AppPaths) -> gtk::Widget {
    let icon_size = 48;
    let icon_path = paths
        .icons_cache_dir()
        .join(format!("{}-{}x{}.png", app.icon_id, 128, 128));
    let image = if icon_path.exists() {
        match gdk::Texture::from_filename(icon_path) {
            Ok(tex) => {
                let img = gtk::Image::from_paintable(Some(&tex));
                img.set_pixel_size(icon_size);
                img
            }
            Err(_) => gtk::Image::from_icon_name("applications-internet"),
        }
    } else {
        gtk::Image::from_icon_name("applications-internet")
    };
    image.set_icon_size(gtk::IconSize::Large);
    image.set_margin_top(4);
    image.set_margin_bottom(4);
    image.set_margin_start(4);
    image.set_margin_end(4);
    image.upcast()
}

fn format_last_launched(app: &WebAppDefinition) -> String {
    match app.last_launched_at {
        Some(ts) => ts
            .format(&Rfc3339)
            .unwrap_or_else(|_| ts.date().to_string()),
        None => "Never".to_string(),
    }
}

fn confirm_reset(handlers: &Handlers, app: &WebAppDefinition) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(&handlers.window)
        .heading(format!("Reset data for {}?", app.name))
        .body("This will clear cookies, storage, cache, and permissions for this web app.")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("reset", "Reset");
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(
        glib::clone!(@strong handlers.clone(), @strong app.clone() => move |d, resp| {
            if resp == "reset" {
                if let Err(err) = run_reset(&handlers, &app) {
                    tracing::error!(target: "ui", "reset failed: {err:?}");
                }
            }
            d.close();
        }),
    );
    dialog.present();
}

fn confirm_remove(handlers: &Handlers, app: &WebAppDefinition) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(&handlers.window)
        .heading(format!("Remove {}?", app.name))
        .body("This will remove the web app, its data, and its launcher.")
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("remove", "Remove");
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(
        glib::clone!(@strong handlers.clone(), @strong app.clone() => move |d, resp| {
            if resp == "remove" {
                if let Err(err) = run_remove(&handlers, &app) {
                    tracing::error!(target: "ui", "remove failed: {err:?}");
                }
            }
            d.close();
        }),
    );
    dialog.present();
}

fn run_reset(handlers: &Handlers, app: &WebAppDefinition) -> Result<()> {
    handlers
        .ctx
        .permissions
        .delete(app.id)
        .context("delete permissions")?;
    handlers
        .ctx
        .paths
        .delete_profile_dir(app.id)
        .context("delete profile dir")?;
    handlers
        .ctx
        .paths
        .delete_icons_for(&app.icon_id)
        .context("delete icons")?;

    refresh_current(handlers);
    Ok(())
}

fn run_remove(handlers: &Handlers, app: &WebAppDefinition) -> Result<()> {
    run_reset(handlers, app)?;
    handlers
        .ctx
        .registry
        .delete(app.id)
        .context("delete app registry")?;

    handlers.apps.borrow_mut().retain(|a| a.id != app.id);

    if let Err(err) = remove_launcher(&desktop_id_for(app)) {
        tracing::warn!(target: "portal", "remove launcher failed: {err:?}");
    }

    refresh_current(handlers);
    Ok(())
}

fn run_launch(handlers: &Handlers, app: &WebAppDefinition) -> Result<()> {
    let mut app_updated = app.clone();
    app_updated.last_launched_at = Some(OffsetDateTime::now_utc());
    handlers.ctx.registry.save(&app_updated)?;
    handlers
        .apps
        .borrow_mut()
        .iter_mut()
        .filter(|a| a.id == app_updated.id)
        .for_each(|a| *a = app_updated.clone());
    refresh_current(handlers);
    // Launch via sitewrap --shell <id>
    let _ = std::process::Command::new("sitewrap")
        .arg("--shell")
        .arg(app_updated.id.to_string())
        .spawn();
    Ok(())
}

fn open_edit_window(handlers: &Handlers, app: &WebAppDefinition) -> Result<()> {
    let app_id = app.id;
    let win = adw::Window::builder()
        .application(&handlers.window.application().unwrap())
        .transient_for(&handlers.window)
        .modal(true)
        .title("Edit Web App")
        .default_width(420)
        .default_height(260)
        .build();

    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    let url_entry = gtk::Entry::builder()
        .text(&app.start_url)
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    let name_entry = gtk::Entry::builder().text(&app.name).build();

    let open_external_switch = gtk::Switch::builder()
        .active(app.behavior.open_external_links)
        .build();
    let show_nav_switch = gtk::Switch::builder()
        .active(app.behavior.show_navigation)
        .build();
    let error_label = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(vec!["error".into()])
        .wrap(true)
        .build();

    let button_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::End)
        .build();
    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::with_label("Save");
    save_btn.set_can_default(true);
    save_btn.grab_default();
    button_row.append(&cancel_btn);
    button_row.append(&save_btn);

    container.append(&gtk::Label::builder().label("URL").xalign(0.0).build());
    container.append(&url_entry);
    container.append(&gtk::Label::builder().label("Name").xalign(0.0).build());
    container.append(&name_entry);

    let open_external_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    open_external_row.append(
        &gtk::Label::builder()
            .label("Open external links in default browser")
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
    open_external_row.append(&open_external_switch);

    let show_nav_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    show_nav_row.append(
        &gtk::Label::builder()
            .label("Show navigation controls")
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
    show_nav_row.append(&show_nav_switch);

    container.append(&open_external_row);
    container.append(&show_nav_row);
    container.append(&error_label);
    container.append(&button_row);
    win.set_content(Some(&container));

    cancel_btn.connect_clicked(glib::clone!(@weak win => move |_| win.close()));

    save_btn.connect_clicked(glib::clone!(@weak win, @weak url_entry, @weak name_entry, @weak open_external_switch, @weak show_nav_switch, @weak error_label, @strong handlers, @strong app_id => move |_| {
        if let Err(err) = handle_edit(&handlers, app_id, &url_entry, &name_entry, &open_external_switch, &show_nav_switch) {
            error_label.set_label(&format!("{err}"));
            return;
        }
        win.close();
    }));

    // Avoid double submit: enter key should not invoke twice; rely on button only.

    win.present();
    Ok(())
}

fn handle_edit(
    handlers: &Handlers,
    app_id: WebAppId,
    url_entry: &gtk::Entry,
    name_entry: &gtk::Entry,
    open_external_switch: &gtk::Switch,
    show_nav_switch: &gtk::Switch,
) -> Result<()> {
    let url_text = url_entry.text().trim().to_string();
    if url_text.is_empty() {
        bail!("Please enter a URL");
    }
    let parsed = normalize_url(&url_text)?;
    let mut name = name_entry.text().trim().to_string();
    if name.is_empty() {
        name = parsed
            .host_str()
            .map(|h| h.to_string())
            .unwrap_or_else(|| "Web App".to_string());
    }

    let mut apps_mut = handlers.apps.borrow_mut();
    let Some(app) = apps_mut.iter_mut().find(|a| a.id == app_id) else {
        bail!("app not found")
    };
    let url_changed = app.start_url != parsed.to_string();
    app.name = name.clone();
    app.start_url = parsed.to_string();
    app.primary_origin = parsed.origin().ascii_serialization();
    app.behavior.open_external_links = open_external_switch.state();
    app.behavior.show_navigation = show_nav_switch.state();

    handlers.ctx.registry.save(app)?;
    drop(apps_mut);
    refresh_current(handlers);

    // background: refetch icon if URL changed, reinstall launcher with new metadata
    let app_clone = handlers
        .apps
        .borrow()
        .iter()
        .find(|a| a.id == app_id)
        .cloned()
        .unwrap();
    let paths = handlers.ctx.paths.clone();
    let (sender, receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    thread::spawn(move || {
        if url_changed {
            if let Ok(url) = Url::parse(&app_clone.start_url) {
                let _ = fetch_and_cache_icon(&url, &app_clone.icon_id, &paths.icons_cache_dir());
            }
        }
        let descriptor = launcher_descriptor_for(&app_clone, &paths);
        let _ = install_launcher(&descriptor);
        let _ = sender.send(());
    });
    let handlers_clone = handlers.clone();
    receiver.attach(None, move |_| {
        refresh_current(&handlers_clone);
        glib::Continue(false)
    });

    Ok(())
}

fn refresh_current(handlers: &Handlers) {
    let filter = handlers.search_entry.text();
    refresh_listbox(&handlers.list, &handlers.apps.borrow(), &filter, handlers);
}

fn open_create_window(
    app: &adw::Application,
    parent: &adw::ApplicationWindow,
    handlers: Handlers,
) -> Result<()> {
    let win = adw::Window::builder()
        .application(app)
        .transient_for(parent)
        .modal(true)
        .title("Create Web App")
        .default_width(420)
        .default_height(260)
        .build();

    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    let url_entry = gtk::Entry::builder()
        .placeholder_text("https://example.com")
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    let name_entry = gtk::Entry::builder().placeholder_text("App name").build();

    let open_external_switch = gtk::Switch::builder().active(true).build();
    let show_nav_switch = gtk::Switch::builder().active(false).build();
    let error_label = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(vec!["error".into()])
        .wrap(true)
        .build();

    let button_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::End)
        .build();
    let cancel_btn = gtk::Button::with_label("Cancel");
    let create_btn = gtk::Button::with_label("Create");
    create_btn.set_can_default(true);
    create_btn.grab_default();
    button_row.append(&cancel_btn);
    button_row.append(&create_btn);

    container.append(&gtk::Label::builder().label("URL").xalign(0.0).build());
    container.append(&url_entry);
    container.append(&gtk::Label::builder().label("Name").xalign(0.0).build());
    container.append(&name_entry);

    let open_external_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    open_external_row.append(
        &gtk::Label::builder()
            .label("Open external links in default browser")
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
    open_external_row.append(&open_external_switch);

    let show_nav_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    show_nav_row.append(
        &gtk::Label::builder()
            .label("Show navigation controls")
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
    show_nav_row.append(&show_nav_switch);

    container.append(&open_external_row);
    container.append(&show_nav_row);
    container.append(&error_label);
    container.append(&button_row);
    win.set_content(Some(&container));

    cancel_btn.connect_clicked(glib::clone!(@weak win => move |_| win.close()));

    let submitting = std::rc::Rc::new(std::cell::Cell::new(false));
    create_btn.connect_clicked(glib::clone!(@weak win, @weak url_entry, @weak name_entry, @weak open_external_switch, @weak show_nav_switch, @weak error_label, @strong handlers, @strong submitting => move |_| {
        if submitting.get() {
            return;
        }
        submitting.set(true);
        if let Err(err) = handle_create(&handlers, &url_entry, &name_entry, &open_external_switch, &show_nav_switch) {
            error_label.set_label(&format!("{err}"));
            submitting.set(false);
            return;
        }
        win.close();
    }));

    // Avoid double submit via Enter by not setting a default widget and guarding re-entry.

    win.present();
    Ok(())
}

fn open_permissions_window_manager(handlers: &Handlers, app: &WebAppDefinition) -> Result<()> {
    let mut store = handlers
        .ctx
        .permissions
        .load(app.id)
        .context("load permissions")?;
    store.get_or_default_mut(&app.primary_origin);

    let window = adw::PreferencesWindow::builder()
        .transient_for(&handlers.window)
        .modal(true)
        .title(format!("Permissions - {}", app.name))
        .default_width(520)
        .default_height(480)
        .build();
    let page = adw::PreferencesPage::builder().title("Permissions").build();

    let store_rc = Rc::new(RefCell::new(store));
    let repo = handlers.ctx.permissions.clone();
    let app_id = app.id;

    fn rebuild_page(
        page: &adw::PreferencesPage,
        store: &Rc<RefCell<sitewrap_model::PermissionStore>>,
        repo: &sitewrap_model::PermissionRepository,
        app_id: WebAppId,
    ) {
        for child in page.children() {
            page.remove(&child);
        }

        let mut origins: Vec<(String, sitewrap_model::PerOriginPermissions)> = store
            .borrow()
            .origins
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        origins.sort_by(|a, b| a.0.cmp(&b.0));

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
        add_origin_row(page, move |origin| {
            store_clone.borrow_mut().get_or_default_mut(origin);
            if let Err(err) = repo_clone.save(app_id, &store_clone.borrow()) {
                tracing::error!(target: "ui", "save permissions failed: {err:?}");
            }
            rebuild_page(&page_clone, &store_clone, &repo_clone, app_id);
        });
    }

    rebuild_page(&page, &store_rc, &repo, app_id);

    window.add(&page);
    window.present();
    Ok(())
}

fn handle_create(
    handlers: &Handlers,
    url_entry: &gtk::Entry,
    name_entry: &gtk::Entry,
    open_external_switch: &gtk::Switch,
    show_nav_switch: &gtk::Switch,
) -> Result<()> {
    let url_text = url_entry.text().trim().to_string();
    if url_text.is_empty() {
        bail!("Please enter a URL");
    }
    let parsed = normalize_url(&url_text)?;
    let mut name = name_entry.text().trim().to_string();
    if name.is_empty() {
        name = parsed
            .host_str()
            .map(|h| h.to_string())
            .unwrap_or_else(|| "Web App".to_string());
    }
    let mut app_def = WebAppDefinition::new(name, parsed.clone());
    app_def.behavior.open_external_links = open_external_switch.state();
    app_def.behavior.show_navigation = show_nav_switch.state();

    handlers.ctx.registry.save(&app_def)?;
    handlers.apps.borrow_mut().push(app_def.clone());
    refresh_current(handlers);

    let paths = handlers.ctx.paths.clone();
    let app_clone = app_def.clone();
    let (sender, receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    thread::spawn(move || {
        if let Ok(url) = Url::parse(&app_clone.start_url) {
            let _ = fetch_and_cache_icon(&url, &app_clone.icon_id, &paths.icons_cache_dir());
        }
        let descriptor = launcher_descriptor_for(&app_clone, &paths);
        let _ = install_launcher(&descriptor);
        let _ = sender.send(());
    });
    receiver.attach(None, move |_| {
        refresh_current(handlers);
        glib::Continue(false)
    });

    Ok(())
}
