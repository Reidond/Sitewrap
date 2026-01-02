use anyhow::Result;
use gio::Resource;
use glib::Bytes;

pub fn register() -> Result<()> {
    let bytes = Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/sitewrap.gresource"
    )));
    let resource = Resource::from_data(&bytes)?;
    resource.register();
    Ok(())
}
