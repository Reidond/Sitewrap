use sitewrap_portal::LauncherDescriptor;

#[test]
fn desktop_id_roundtrip() {
    let desc = LauncherDescriptor {
        desktop_id: "xyz.andriishafar.Sitewrap.webapp.123.desktop".into(),
        name: "Test".into(),
        exec: "sitewrap --shell 123".into(),
        icon_name: "xyz.andriishafar.Sitewrap.webapp.123".into(),
        icon_file: Some(std::path::PathBuf::from("/tmp/icon.png")),
    };
    assert!(desc.desktop_id.ends_with(".desktop"));
    assert!(desc.icon_name.starts_with("xyz.andriishafar.Sitewrap"));
    assert!(desc.icon_file.is_some());
}
