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
    if let Ok(url) = Url::parse(&origin) {
        if let Some(host) = url.host_str() {
            group.set_title(host);
        }
    }
    group.set_description(Some("Website origin"));

    let options = gtk::StringList::new(&["Ask", "Allow", "Block"]);
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&options)
        .build();

    let icon_name = match field {
        PermissionField::Notifications => "preferences-system-notifications-symbolic",
        PermissionField::Camera => "camera-video-symbolic",
        PermissionField::Microphone => "audio-input-microphone-symbolic",
        PermissionField::Location => "find-location-symbolic",
    };
    row.add_prefix(&gtk::Image::from_icon_name(icon_name));

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
    let group = adw::PreferencesGroup::builder()
        .title("Add Origin")
        .description("Allow additional websites to request permissions")
        .build();

    let row = adw::EntryRow::builder()
        .title("Website URL")
        .input_purpose(gtk::InputPurpose::Url)
        .build();

    let button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();

    let build_group = Rc::new(build_group);
    let logic = {
        let build_group = build_group.clone();
        move |row: &adw::EntryRow| {
            let text = row.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            let parsed = Url::parse(&text).or_else(|_| Url::parse(&format!("https://{text}")));
            match parsed {
                Ok(url) => {
                    let origin = url.origin().ascii_serialization();
                    (build_group)(&origin);
                    row.set_text("");
                }
                Err(err) => {
                    tracing::warn!(target: "ui", "invalid origin entered: {err}");
                }
            }
        }
    };

    let logic_rc = Rc::new(logic);

    let l = logic_rc.clone();
    row.connect_entry_activated(move |r| l(r));

    let l = logic_rc.clone();
    button.connect_clicked(glib::clone!(@weak row => move |_| l(&row)));

    row.add_suffix(&button);
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
