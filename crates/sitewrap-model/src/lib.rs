use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

pub type WebAppId = Uuid;

/// Centralized access to sandboxed XDG locations.
#[derive(Debug, Clone)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
}

impl AppPaths {
    pub fn new() -> Result<Self> {
        let dirs = BaseDirs::new().context("unable to resolve XDG base directories")?;
        Ok(Self {
            config_dir: dirs.config_dir().join("sitewrap"),
            data_dir: dirs.data_dir().join("sitewrap"),
            cache_dir: dirs.cache_dir().join("sitewrap"),
        })
    }

    #[cfg(test)]
    pub fn for_test(root: PathBuf) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
        }
    }

    pub fn apps_dir(&self) -> PathBuf {
        self.config_dir.join("apps")
    }

    pub fn permissions_dir(&self) -> PathBuf {
        self.config_dir.join("permissions")
    }

    pub fn icons_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("icons")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.data_dir.join("profiles")
    }

    pub fn profile_dir(&self, id: WebAppId) -> PathBuf {
        self.profiles_dir().join(id.to_string())
    }

    pub fn delete_profile_dir(&self, id: WebAppId) -> Result<()> {
        let dir = self.profile_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).with_context(|| format!("remove profile dir {dir:?}"))?;
        }
        Ok(())
    }

    pub fn delete_icons_for(&self, icon_id: &str) -> Result<()> {
        let dir = self.icons_cache_dir();
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&dir).with_context(|| format!("scan icons dir {dir:?}"))? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(icon_id) {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "default_open_external_links")]
    pub open_external_links: bool,
    #[serde(default = "default_show_navigation")]
    pub show_navigation: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            open_external_links: default_open_external_links(),
            show_navigation: default_show_navigation(),
        }
    }
}

fn default_open_external_links() -> bool {
    true
}

fn default_show_navigation() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAppDefinition {
    pub id: WebAppId,
    pub name: String,
    pub start_url: String,
    pub primary_origin: String,
    pub icon_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub last_launched_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub behavior: BehaviorConfig,
}

impl WebAppDefinition {
    pub fn new(name: String, start_url: Url) -> Self {
        let id = Uuid::new_v4();
        let primary_origin = start_url.origin().ascii_serialization();
        let now = OffsetDateTime::now_utc();
        Self {
            id,
            name,
            start_url: start_url.to_string(),
            primary_origin,
            icon_id: format!("xyz.andriishafar.Sitewrap.webapp.{id}"),
            created_at: now,
            last_launched_at: None,
            behavior: BehaviorConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    #[default]
    Ask,
    Allow,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerOriginPermissions {
    #[serde(default)]
    pub notifications: PermissionState,
    #[serde(default)]
    pub camera: PermissionState,
    #[serde(default)]
    pub microphone: PermissionState,
    #[serde(default)]
    pub location: PermissionState,
}

impl Default for PerOriginPermissions {
    fn default() -> Self {
        Self {
            notifications: PermissionState::Ask,
            camera: PermissionState::Ask,
            microphone: PermissionState::Ask,
            location: PermissionState::Ask,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionStore {
    #[serde(flatten)]
    pub origins: HashMap<String, PerOriginPermissions>,
}

impl PermissionStore {
    pub fn get_or_default_mut(&mut self, origin: &str) -> &mut PerOriginPermissions {
        self.origins.entry(origin.to_string()).or_default()
    }
}

#[derive(Clone)]
pub struct AppRegistry {
    paths: AppPaths,
}

impl AppRegistry {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn list(&self) -> Result<Vec<WebAppDefinition>> {
        let dir = self.paths.apps_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut apps = Vec::new();
        for entry in fs::read_dir(&dir).with_context(|| format!("reading apps dir {dir:?}"))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            if let Ok(app) = self.load_from_path(&path) {
                apps.push(app);
            }
        }
        Ok(apps)
    }

    pub fn load(&self, id: WebAppId) -> Result<WebAppDefinition> {
        let path = self.app_path(id);
        self.load_from_path(&path)
    }

    pub fn save(&self, app: &WebAppDefinition) -> Result<()> {
        let path = self.app_path(app.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create apps dir {parent:?}"))?;
        }
        let toml = toml::to_string_pretty(app).context("serialize app definition")?;
        fs::write(&path, toml).with_context(|| format!("write app file {path:?}"))?;
        Ok(())
    }

    pub fn delete(&self, id: WebAppId) -> Result<()> {
        let path = self.app_path(id);
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("remove app file {path:?}"))?;
        }
        Ok(())
    }

    fn app_path(&self, id: WebAppId) -> PathBuf {
        self.paths.apps_dir().join(format!("{id}.toml"))
    }

    fn load_from_path(&self, path: &PathBuf) -> Result<WebAppDefinition> {
        let data = fs::read_to_string(path).with_context(|| format!("read app file {path:?}"))?;
        let app: WebAppDefinition =
            toml::from_str(&data).with_context(|| format!("parse app file {path:?}"))?;
        Ok(app)
    }
}

#[derive(Clone)]
pub struct PermissionRepository {
    paths: AppPaths,
}

impl PermissionRepository {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn load(&self, id: WebAppId) -> Result<PermissionStore> {
        let path = self.permission_path(id);
        if !path.exists() {
            return Ok(PermissionStore::default());
        }
        let data =
            fs::read_to_string(&path).with_context(|| format!("read permission file {path:?}"))?;
        let store: PermissionStore = toml::from_str(&data).context("parse permission file")?;
        Ok(store)
    }

    pub fn save(&self, id: WebAppId, store: &PermissionStore) -> Result<()> {
        let path = self.permission_path(id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create permissions dir {parent:?}"))?;
        }
        let data = toml::to_string_pretty(store).context("serialize permission store")?;
        fs::write(&path, data).with_context(|| format!("write permission file {path:?}"))?;
        Ok(())
    }

    pub fn delete(&self, id: WebAppId) -> Result<()> {
        let path = self.permission_path(id);
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("remove permission file {path:?}"))?;
        }
        Ok(())
    }

    fn permission_path(&self, id: WebAppId) -> PathBuf {
        self.paths.permissions_dir().join(format!("{id}.toml"))
    }
}

/// Convenience helper to validate and normalize URLs.
pub fn normalize_url(input: &str) -> Result<Url> {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    let with_scheme = if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = Url::parse(&with_scheme).context("invalid URL")?;
    Ok(parsed)
}

/// Returns the canonical primary origin string for storage/comparison.
pub fn origin_for(url: &Url) -> String {
    url.origin().ascii_serialization()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn permission_defaults() {
        let store = PermissionStore::default();
        assert_eq!(store.origins.len(), 0);
        let mut store = store;
        let origin = "https://example.com";
        let entry = store.get_or_default_mut(origin);
        assert_eq!(entry.notifications, PermissionState::Ask);
        assert_eq!(entry.camera, PermissionState::Ask);
    }

    #[test]
    fn normalize_url_adds_scheme() {
        let url = normalize_url("example.com").unwrap();
        assert_eq!(url.as_str(), "https://example.com/");

        let url2 = normalize_url("http://example.com").unwrap();
        assert_eq!(url2.as_str(), "http://example.com/");
    }

    #[test]
    fn behavior_flags_roundtrip() {
        let root = std::env::temp_dir().join(format!("sitewrap-test-behavior-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::for_test(root.clone());
        let registry = AppRegistry::new(paths.clone());

        let mut app =
            WebAppDefinition::new("Example".into(), Url::parse("https://example.com").unwrap());
        app.behavior.open_external_links = false;
        app.behavior.show_navigation = true;
        registry.save(&app).unwrap();

        let loaded = registry.load(app.id).unwrap();
        assert!(!loaded.behavior.open_external_links);
        assert!(loaded.behavior.show_navigation);

        registry.delete(app.id).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_save_load_delete() {
        let root = std::env::temp_dir().join(format!("sitewrap-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::for_test(root.clone());
        let registry = AppRegistry::new(paths.clone());

        let app =
            WebAppDefinition::new("Example".into(), Url::parse("https://example.com").unwrap());
        registry.save(&app).unwrap();

        let loaded = registry.load(app.id).unwrap();
        assert_eq!(loaded.name, "Example");
        assert_eq!(loaded.start_url, "https://example.com/");

        registry.delete(app.id).unwrap();
        let app_path = paths.apps_dir().join(format!("{}.toml", app.id));
        assert!(!app_path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn permission_repo_roundtrip() {
        let root = std::env::temp_dir().join(format!("sitewrap-test-perm-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::for_test(root.clone());
        let repo = PermissionRepository::new(paths.clone());
        let app_id = Uuid::new_v4();
        let mut store = PermissionStore::default();
        store
            .get_or_default_mut("https://example.com")
            .notifications = PermissionState::Allow;
        repo.save(app_id, &store).unwrap();

        let loaded = repo.load(app_id).unwrap();
        assert_eq!(loaded.origins.len(), 1);
        assert_eq!(
            loaded
                .origins
                .get("https://example.com")
                .unwrap()
                .notifications,
            PermissionState::Allow
        );

        repo.delete(app_id).unwrap();
        let perm_path = paths.permissions_dir().join(format!("{}.toml", app_id));
        assert!(!perm_path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn behavior_defaults_match_spec() {
        let app =
            WebAppDefinition::new("Example".into(), Url::parse("https://example.com").unwrap());
        assert!(app.behavior.open_external_links);
        assert!(!app.behavior.show_navigation);
    }

    #[test]
    fn origin_for_matches_normalize() {
        let url = Url::parse("https://Example.com/path").unwrap();
        let norm = normalize_url("Example.com").unwrap();
        assert_eq!(origin_for(&url), norm.origin().ascii_serialization());
    }
}
