use anyhow::Result;
use gtk4::gio;
use gtk4::glib;

pub fn register() -> Result<()> {
    let bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/sitewrap.gresource"
    )));
    let resource = gio::Resource::from_data(&bytes)?;
    gio::resources_register(&resource);
    Ok(())
}
