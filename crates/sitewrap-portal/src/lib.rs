use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use ashpd::desktop::Icon;
use ashpd::desktop::{dynamic_launcher, file_chooser, notification, open_uri};
use ashpd::url::Url;
use once_cell::sync::Lazy;
use thiserror::Error;
use tokio::runtime::Runtime;
use tracing::{info, warn};

static RUNTIME: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("tokio runtime"));

#[derive(Debug, Clone)]
pub struct LauncherDescriptor {
    /// Desktop file id used by the portal (e.g., xyz.andriishafar.Sitewrap.webapp.<uuid>.desktop)
    pub desktop_id: String,
    pub name: String,
    pub exec: String,
    pub icon_name: String,
    pub icon_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct NotificationRequest {
    pub app_id: String,
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SaveFileRequest {
    pub title: String,
    pub suggested_name: String,
    pub default_directory: Option<PathBuf>,
    pub content: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum PortalError {
    #[error("required portal backend unavailable")]
    Unavailable,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn icon_from_path(path: &PathBuf) -> Result<Icon> {
    let bytes = fs::read(path).with_context(|| format!("read icon {path:?}"))?;
    Ok(Icon::Bytes(bytes))
}

fn desktop_entry_from_descriptor(descriptor: &LauncherDescriptor) -> String {
    format!(
        "[Desktop Entry]\nName={}\nExec={}\nType=Application\nIcon={}\nCategories=Network;WebBrowser;\n",
        descriptor.name, descriptor.exec, descriptor.icon_name
    )
}

pub fn install_launcher(descriptor: &LauncherDescriptor) -> Result<()> {
    info!(target: "portal", desktop_id = %descriptor.desktop_id, "install launcher via DynamicLauncher portal");
    let icon = descriptor
        .icon_file
        .as_ref()
        .and_then(|path| match icon_from_path(path) {
            Ok(icon) => Some(icon),
            Err(err) => {
                warn!(
                    target: "portal",
                    path = %path.display(),
                    error = %err,
                    "failed to read icon; using empty icon"
                );
                None
            }
        })
        .unwrap_or_else(|| Icon::Bytes(Vec::new()));

    let desktop_entry = desktop_entry_from_descriptor(descriptor);

    RUNTIME.block_on(async {
        let proxy = dynamic_launcher::DynamicLauncherProxy::new()
            .await
            .context("connect DynamicLauncher portal")?;
        let options = dynamic_launcher::PrepareInstallOptions::default()
            .launcher_type(dynamic_launcher::LauncherType::WebApplication);
        let response = proxy
            .prepare_install(None, &descriptor.name, icon, options)
            .await
            .context("prepare DynamicLauncher install")?
            .response()
            .context("read DynamicLauncher prepare response")?;
        let token = response.token();
        proxy
            .install(token, &descriptor.desktop_id, &desktop_entry)
            .await
            .context("install desktop entry via DynamicLauncher portal")?;
        Ok::<_, anyhow::Error>(())
    })
}

pub fn update_launcher(descriptor: &LauncherDescriptor) -> Result<()> {
    install_launcher(descriptor)
}

pub fn remove_launcher(desktop_id: &str) -> Result<()> {
    info!(target: "portal", desktop_id, "remove launcher via DynamicLauncher portal");
    RUNTIME.block_on(async {
        let proxy = dynamic_launcher::DynamicLauncherProxy::new()
            .await
            .context("connect DynamicLauncher portal")?;
        proxy
            .uninstall(desktop_id)
            .await
            .context("uninstall desktop entry via DynamicLauncher portal")?;
        Ok::<_, anyhow::Error>(())
    })
}

pub fn send_notification(request: &NotificationRequest) -> Result<()> {
    info!(target: "portal", app_id = %request.app_id, title = %request.title, "send notification via portal");
    RUNTIME.block_on(async {
        let proxy = notification::NotificationProxy::new()
            .await
            .context("connect Notification portal")?;
        let note = notification::Notification::new(&request.title).body(request.body.as_str());
        let note = if let Some(icon) = &request.icon {
            note.icon(Icon::with_names([icon.as_str()]))
        } else {
            note
        };
        proxy
            .add_notification(&request.app_id, note)
            .await
            .context("send notification via portal")?;
        Ok::<_, anyhow::Error>(())
    })
}

pub fn open_uri(uri: &str) -> Result<()> {
    info!(target: "portal", uri, "open uri via portal");
    RUNTIME.block_on(async {
        let uri = Url::parse(uri).context("parse URI")?;
        open_uri::OpenFileRequest::default()
            .send_uri(&uri)
            .await
            .context("send OpenURI request")?
            .response()
            .context("read OpenURI response")?;
        Ok::<_, anyhow::Error>(())
    })
}

pub fn save_file(request: &SaveFileRequest) -> Result<()> {
    info!(target: "portal", file = %request.suggested_name, "save file via FileChooser portal");
    RUNTIME.block_on(async {
        let proxy = match file_chooser::FileChooserProxy::new().await {
            Ok(p) => p,
            Err(err) => return Err(PortalError::Unavailable.into()).context(err),
        };
        let mut options = file_chooser::SaveFileOptions::default()
            .accept_label("Save")
            .modal(true)
            .current_name(&request.suggested_name);

        if let Some(dir) = &request.default_directory {
            options = options.current_folder(dir);
        }

        let response = proxy
            .save_file(Some(&request.title), options)
            .await
            .context("open SaveFile portal")?
            .response()
            .context("read SaveFile response")?;

        let Some(file) = response.selected_file() else {
            return Ok(()); // cancelled
        };

        if let Some(path) = file.path() {
            std::fs::write(path, &request.content).context("write selected file")?;
        } else {
            // Fallback: attempt portal-provided writer
            let mut writer = file.write().await.context("open portal writer")?;
            use tokio::io::AsyncWriteExt;
            writer
                .write_all(&request.content)
                .await
                .context("write portal file")?;
            writer.flush().await.context("flush portal file")?;
        }
        Ok::<_, anyhow::Error>(())
    })
}

fn file_chooser_available() -> bool {
    RUNTIME.block_on(async { file_chooser::FileChooserProxy::new().await.is_ok() })
}

async fn open_uri_portal_available() -> bool {
    let Ok(connection) = ashpd::zbus::Connection::session().await else {
        return false;
    };

    let proxy = match ashpd::zbus::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.OpenURI",
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => return false,
    };

    proxy.get_property::<u32>("version").await.is_ok()
}

pub fn is_supported() -> bool {
    RUNTIME.block_on(async {
        dynamic_launcher::DynamicLauncherProxy::new().await.is_ok()
            && notification::NotificationProxy::new().await.is_ok()
            && open_uri_portal_available().await
    })
}

pub fn is_open_uri_supported() -> bool {
    RUNTIME.block_on(open_uri_portal_available())
}

pub fn is_file_chooser_supported() -> bool {
    file_chooser_available()
}

pub fn warn_if_stubbed() {
    if !is_supported() {
        warn!(target: "portal", "xdg-desktop-portal not available; host integration is disabled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_matches_descriptor() {
        let descriptor = LauncherDescriptor {
            desktop_id: "xyz.andriishafar.Sitewrap.webapp.123.desktop".into(),
            name: "Demo App".into(),
            exec: "sitewrap --shell 123".into(),
            icon_name: "xyz.andriishafar.Sitewrap.webapp.123".into(),
            icon_file: None,
        };

        let entry = desktop_entry_from_descriptor(&descriptor);
        let expected = "[Desktop Entry]\nName=Demo App\nExec=sitewrap --shell 123\nType=Application\nIcon=xyz.andriishafar.Sitewrap.webapp.123\nCategories=Network;WebBrowser;\n";
        assert_eq!(entry, expected);
    }
}
