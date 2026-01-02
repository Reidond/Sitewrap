use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::glib;
use sitewrap_model::{
    PerOriginPermissions, PermissionRepository, PermissionState, PermissionStore, WebAppId,
};
use url::Url;

#[derive(Clone, Copy)]
pub enum PermissionField {
    Notifications,
    Camera,
    Microphone,
    Location,
}

#[allow(clippy::too_many_arguments)]
pub fn add_permission_row(
    group: &adw::PreferencesGroup,
    title: &str,
    current: PermissionState,
    field: PermissionField,
    store: Rc<RefCell<PermissionStore>>,
    origin: String,
    repo: PermissionRepository,
    app_id: WebAppId,
) {
    let options = gtk::StringList::new(&["Ask", "Allow", "Block"]);
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&options)
        .build();
    row.set_selected(permission_state_to_index(current));
    row.connect_selected_notify(move |row| {
        let state = index_to_permission_state(row.selected());
        let mut store = store.borrow_mut();
        let entry = store.get_or_default_mut(&origin);
        set_permission_field(entry, field, state);
        if let Err(err) = repo.save(app_id, &store) {
            tracing::error!(target: "ui", "save permissions failed: {err:?}");
        }
    });
    group.add(&row);
}

pub fn add_origin_row(page: adw::PreferencesPage, build_group: impl Fn(&str) + 'static) {
    let row = adw::ActionRow::builder()
        .title("Add origin")
        .subtitle("Enter a site URL (saved immediately)")
        .build();

    let entry = gtk::Entry::builder()
        .placeholder_text("https://example.com")
        .hexpand(true)
        .build();
    let button = gtk::Button::with_label("Add");

    button.connect_clicked(glib::clone!(@weak entry => move |_| {
        let text = entry.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let parsed = Url::parse(&text).or_else(|_| Url::parse(&format!("https://{text}")));
        match parsed {
            Ok(url) => {
                let origin = url.origin().ascii_serialization();
                build_group(&origin);
            }
            Err(err) => {
                tracing::warn!(target: "ui", "invalid origin entered: {err}");
            }
        }
        entry.set_text("");
    }));

    let hbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    hbox.append(&entry);
    hbox.append(&button);

    row.add_suffix(&hbox);
    let group = adw::PreferencesGroup::builder().build();
    group.add(&row);
    page.add(&group);
}

fn set_permission_field(
    entry: &mut PerOriginPermissions,
    field: PermissionField,
    state: PermissionState,
) {
    match field {
        PermissionField::Notifications => entry.notifications = state,
        PermissionField::Camera => entry.camera = state,
        PermissionField::Microphone => entry.microphone = state,
        PermissionField::Location => entry.location = state,
    }
}

fn permission_state_to_index(state: PermissionState) -> u32 {
    match state {
        PermissionState::Ask => 0,
        PermissionState::Allow => 1,
        PermissionState::Block => 2,
    }
}

fn index_to_permission_state(index: u32) -> PermissionState {
    match index {
        1 => PermissionState::Allow,
        2 => PermissionState::Block,
        _ => PermissionState::Ask,
    }
}
