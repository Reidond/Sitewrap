# Sitewrap Portal Integration (runtime expectations)

- Requires a running `xdg-desktop-portal` service (DBus session) for DynamicLauncher, OpenURI, Notification, and FileChooser APIs.
- If the portal backend is unavailable, calls will return errors and UI layers should remain resilient; DynamicLauncher/Notification/OpenURI will no-op.
- No build-time dependencies beyond `ashpd` (already in Cargo.toml); runtime availability determines feature behavior.
- Env vars: none required; the portal detects desktop backend via DBus.
