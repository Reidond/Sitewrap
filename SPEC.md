# SPEC: Sitewrap Web App Manager (Flatpak, Rust, Chromium/CEF)

- **Document ID:** SPEC-WAM-001
- **Status:** Draft
- **Target Platforms:** Linux (GNOME-first; functional on other desktops where portals exist)
- **Packaging:** Flatpak (sandboxed)
- **Flatpak App ID:** xyz.andriishafar.Sitewrap
- **UI Toolkit:** GTK 4 + libadwaita (GNOME HIG), UI authored in Blueprint (.blp)
- **Language:** Rust (latest stable toolchain at build time; see §10.3)

---

## 1. Problem Statement

Users want to run any website as if it were a native application on Linux: separate launcher, separate icon, separate profile (cookies/storage), and predictable permission behavior (e.g., notifications prompting like a browser), while remaining sandboxed and aligned with GNOME UX.

---

## 2. Product Summary

**Sitewrap** is a Flatpak-delivered graphical application that:

1. Lets the user **create “Web Apps” from arbitrary URLs**.
2. Installs each Web App as a **separate desktop entry** with a **favicon-derived icon**.
3. Launches Web Apps in a dedicated **application shell mode** (minimal chrome), using a **Chromium-based engine (CEF)**.
4. Stores all internal configuration and profiles under the app’s **XDG directories**, while exporting desktop integration through **portals** to the user’s host desktop environment.
5. Implements a **browser-like permission model**, including notifications.

---

## 3. Goals

### 3.1 Must-Have Goals
- **Flatpak sandboxing** as the primary security boundary.
- **Write to XDG app directories** inside the sandbox:
  - `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` (Flatpak-mapped).
- **Create per-web-app desktop entries and icons** visible to the host desktop.
- **Chromium/CEF engine** for rendering and web APIs.
- **Rust implementation** using current stable Rust.
- **GNOME-quality UI**:
  - GTK4, libadwaita, GNOME HIG adherence.
  - UI defined via Blueprint and compiled at build time.
- **Two modes in one product**:
  - **Manager mode** (default): create/manage web apps.
  - **Shell mode**: run a specific web app in a dedicated window.
- **Browser-style permission model**, including:
  - Notifications prompting and per-site persistence.

### 3.2 Nice-to-Have Goals (Optional in v1)
- Export/Import web app definitions.
- Global policy presets (e.g., block all notifications globally).
- Sync/backup of app definitions (not browsing data).

---

## 4. Non-Goals (v1)

- Not a full general-purpose browser replacement (no tabbed browsing requirement).
- Not building a separate Flatpak per website (no per-site packaged app).
- Not providing background Web Push delivery when the app is fully closed (can be evaluated later; see §14).

---

## 5. Key User Stories

1. **Create a web app**
   - As a user, I can enter a URL, name, and optionally choose an icon.
   - The app installs a launcher in my application menu.

2. **Launch a web app**
   - As a user, clicking the launcher opens the site in a minimal “app-like” window.

3. **Manage web apps**
   - As a user, I can edit name/icon/URL, reset data, or remove the web app entirely.

4. **Permission prompting**
   - As a user, when a site asks to show notifications, I get a prompt (Allow/Block).
   - My choice is remembered per site.

5. **Notifications**
   - As a user, allowed notifications appear as system notifications.
   - Clicking a notification focuses (or launches) the corresponding web app.

---

## 6. Functional Requirements

### 6.1 Manager Mode (Default UI)
**FR-M1**: Display a list/grid of existing web apps with:
- Name
- Primary URL/origin
- Icon
- Last launched timestamp (optional)

**FR-M2**: Provide “Create New Web App” flow:
- Input: URL (required), Name (required), Icon (auto from site; editable)
- Options (v1 minimal):
  - Open external links in default browser (default: on)
  - Show navigation controls (default: off/minimal)
  - Start at URL (default: provided URL)

**FR-M3**: Provide management actions per web app:
- Launch
- Edit (name, URL, icon)
- Permissions (view/edit)
- Reset data (clear cookies/storage/cache for that web app)
- Remove (uninstall launcher + remove profile data)

**FR-M4**: Search/filter web apps by name and/or URL.

---

### 6.2 Web App Shell Mode (Per-App Window)
**FR-S1**: Launching a created desktop entry opens a dedicated window for that web app:
- Loads configured start URL
- Uses that app’s isolated profile data directory
- Uses that app’s permissions store

**FR-S2**: Minimal window chrome consistent with GNOME:
- libadwaita header bar
- App icon + name
- App menu with:
  - Reload
  - Copy link
  - Open in default browser
  - Permissions
  - Clear data (optional)
  - About (optional per-app info)

**FR-S3**: External link handling:
- If navigation leaves the “primary origin” and policy is “external”, open via OpenURI portal.
- Otherwise allow in-app navigation (configurable).

---

### 6.3 Desktop Integration (Host Visible)
Because the app is sandboxed, **all host desktop file/icon installation MUST be performed via portals**, not raw writes to `~/.local/share`.

**FR-D1**: Create a host-visible launcher for each web app:
- Desktop entry name: user-visible app name
- Exec: launches the Flatpak app in shell mode for that web app
- Icon: installed as a themed icon (preferred) or absolute icon path (fallback)

**FR-D2**: Uninstall launcher & icon when removing the web app.

**FR-D3**: Update launcher metadata (name/icon) when user edits.

**Implementation note (portal requirement):**
- Use **xdg-desktop-portal DynamicLauncher** to create/install desktop entries and icon assets.
- Store internal app definition inside the Flatpak sandbox; the desktop entry is a “pointer” back to the Flatpak app.

---

### 6.4 Icon Acquisition (Favicon → App Icon)
**FR-I1**: On create/edit, fetch best available icons in priority order:
1. `<link rel="icon" sizes=...>`
2. `<link rel="apple-touch-icon" ...>`
3. `/favicon.ico`
4. Fallback: generated icon with site hostname initials

**FR-I2**: Normalize into PNG icons at standard sizes:
- 16, 32, 48, 64, 128, 256, 512

**FR-I3**: Store icon sources in sandbox, export chosen icon to host via DynamicLauncher.

---

### 6.5 Profile Isolation
**FR-P1**: Each web app has an isolated web profile directory (cookies, local storage, cache):
- Path under sandboxed XDG data directory
- No shared cookies/storage across web apps by default

**FR-P2**: Reset data deletes that web app’s profile directory (with confirmation).

---

## 7. Permission Model Requirements (Browser-Like)

### 7.1 Permission Types (v1)
At minimum, v1 MUST support:
- **Notifications** (explicit requirement)

v1 SHOULD lay groundwork for:
- Microphone
- Camera
- Location
- Clipboard read/write (where possible)
- Persistent storage

### 7.2 Permission States
For each `{web_app_id, origin, permission_type}` store:
- `ask` (default)
- `allow`
- `block`

### 7.3 Prompting UX
**FR-PR1**: When a site requests a permission and state is `ask`:
- Show an in-app prompt (AdwMessageDialog / AdwToast + dialog) with:
  - “Allow”
  - “Block”
  - Optional “Remember this decision” (default: on)

**FR-PR2**: Provide a “Permissions” page per web app:
- List relevant origins (at least the primary origin)
- For each supported permission type, a dropdown: Ask / Allow / Block

**FR-PR3**: Changing permissions takes effect immediately for new requests; existing grants may require reload.

---

## 8. Notifications Requirements (Explicit)

### 8.1 Web Permission Logic
**FR-N1**: When a page calls the Notifications API:
- If permission is `allow`, proceed.
- If `block`, deny.
- If `ask`, prompt user (see §7.3) and persist result.

### 8.2 Delivery Mechanism
**FR-N2**: Notifications MUST be delivered as system notifications using the portal:
- Use **org.freedesktop.portal.Notification** (Notification portal)
- Notification content includes:
  - App name
  - Site-provided title/body (sanitized)
  - App icon (the exported icon)
  - Optional site favicon badge (optional)

### 8.3 Activation Behavior
**FR-N3**: Clicking a notification MUST focus the corresponding web app window if running; otherwise launch it:
- Requires storing a stable notification “activation token” mapping to `{web_app_id}`.

### 8.4 Privacy/Safety Constraints
**FR-N4**: Provide a per-web-app setting:
- “Allow notifications” (Ask/Allow/Block, default Ask)
- “Mute notifications” quick toggle (optional v1)

---

## 9. Architecture

### 9.1 High-Level Components
1. **UI Layer (GTK4/libadwaita)**
   - Manager window
   - App creation/edit dialogs
   - Permissions UI
   - Shell window chrome

2. **Web Engine Layer (CEF)**
   - Embedded Chromium via CEF
   - Per-web-app profile directories
   - Request/permission handlers

3. **Portal Integration Layer**
   - DynamicLauncher portal for desktop entries/icons
   - Notification portal for system notifications
   - OpenURI portal for external links
   - FileChooser portal (recommended for downloads)

4. **Persistence Layer**
   - App registry (definitions)
   - Permission store
   - Metadata cache (icons, last-launched, etc.)

---

### 9.2 Process Model
CEF is multiprocess.

- **Main process** (Rust GTK UI)
  - Runs Manager UI OR Shell UI depending on CLI args
  - Owns portal calls
  - Owns permission store & prompts

- **CEF subprocesses**
  - Renderer, GPU, utility as required by CEF
  - Use per-app profile paths supplied by main process

---

### 9.3 App Definition Data Model
Each created web app is stored as an internal record:

```toml
# $XDG_CONFIG_HOME/sitewrap/apps/<web_app_id>.toml
id = "<uuid>"
name = "Example App"
start_url = "https://example.com/"
primary_origin = "https://example.com"
icon_id = "xyz.andriishafar.Sitewrap.webapp.<uuid>" # exported icon name
created_at = 2025-12-29T00:00:00Z
last_launched_at = 2025-12-29T00:00:00Z

[behavior]
open_external_links = true
show_navigation = false
```

Permissions store example:

```toml
# $XDG_CONFIG_HOME/sitewrap/permissions/<web_app_id>.toml
["https://example.com"]
notifications = "ask" # ask|allow|block
camera = "ask"
microphone = "ask"
location = "ask"
```

---

### 9.4 CLI Contract (for .desktop Exec)
The desktop entry MUST be able to launch a specific web app deterministically:

- `sitewrap --shell <web_app_id>`
  - Starts Shell mode and loads the web app definition.

Optional:
- `sitewrap --manager`
  - Forces manager mode.

---

## 10. Technology Requirements

### 10.1 Flatpak + Sandboxing
**TR-F1**: Application is distributed as a Flatpak.
**TR-F2**: Internal writes are restricted to the Flatpak app’s XDG directories.
**TR-F3**: Host desktop integration is done via portals (DynamicLauncher), not raw filesystem permissions.

Recommended Flatpak permissions:
- `--share=network`
- `--socket=wayland` and fallback `--socket=fallback-x11`
- `--device=dri` (for GPU acceleration)
- No broad `--filesystem=home` access

### 10.2 UI Toolkit
**TR-U1**: GTK 4 + libadwaita.
**TR-U2**: GNOME HIG conventions (header bars, adaptive layouts, dialogs).
**TR-U3**: UI defined in Blueprint `.blp` and compiled as part of the build, shipped as GResources.

### 10.3 Rust Toolchain
**TR-R1**: Rust stable (latest at build/release time).
**TR-R2**: Enforce formatting and linting:
- `rustfmt`, `clippy` (CI gating)
**TR-R3**: Prefer safe Rust; isolate `unsafe` in minimal FFI modules for CEF.

### 10.4 Web Engine
**TR-W1**: Chromium-based engine via **CEF**.
**TR-W2**: Per-web-app profile paths provided to CEF for storage isolation.
**TR-W3**: Integrate CEF permission callbacks with app permission store and GNOME-style prompts.

---

## 11. Build and Resource Pipeline

- Blueprint:
  - `.blp` → compiled to `.ui` via `blueprint-compiler`
- Resources:
  - `.ui`, icons, CSS → bundled into GResource
- Rust build:
  - `cargo build` (with a build script or Meson wrapper to compile Blueprint/resources)
- Flatpak build:
  - Flatpak manifest pulls:
    - GNOME runtime/SDK
    - Rust toolchain extension
    - CEF binaries or build artifacts

---

## 12. UX Requirements (GNOME HIG / libadwaita)

### 12.1 Manager Window (Suggested Layout)
- `AdwApplicationWindow`
- `AdwHeaderBar` with:
  - App title
  - “+” (Create)
  - Search
- Content:
  - `AdwNavigationSplitView` (or adaptive equivalent)
    - Sidebar: list of web apps
    - Detail page: selected web app settings/actions

### 12.2 Create Web App Flow
- Step 1: URL entry (validate scheme, normalize)
- Step 2: Name suggestion (from HTML title / host)
- Step 3: Icon preview (auto; allow override)
- Step 4: Create (installs launcher)

### 12.3 Shell Window
- Minimal header bar
- Optional navigation controls
- Menu includes permissions/settings actions

Accessibility:
- Keyboard navigation for all controls
- Proper ATK/AT-SPI labeling
- High contrast compliance via Adwaita theming

---

## 13. Security and Privacy Considerations

- Flatpak sandbox is mandatory; do not request broad filesystem access.
- Prefer portals for all host integration.
- Store permissions per origin; default `ask`.
- Provide “Clear Data” and “Remove Web App” flows with confirmations.
- Ensure notification content sanitization (avoid markup injection, limit length).

Chromium sandboxing note:
- Where feasible, keep Chromium/CEF sandbox features enabled.
- If disabling Chromium sandbox is required in some environments, the app MUST:
  - Warn in logs, and
  - Document the security trade-off.

---

## 14. Risks and Mitigations

**R1: CEF + GTK4 integration complexity**
- Mitigation: define a CEF abstraction layer; prototype early.
- Consider OSR rendering path if direct embedding is problematic.

**R2: Portal availability differences**
- Mitigation: detect portal support at runtime and provide fallback guidance.
- If DynamicLauncher portal is unavailable, provide a manual export option (save .desktop + icon) as fallback (optional).

**R3: Notification + activation behavior variance**
- Mitigation: keep activation minimal and robust; store stable IDs.

---

## 15. Acceptance Criteria (v1)

A build is “v1-acceptable” when:

1. Flatpak installs and runs on GNOME.
2. User can create a web app from an arbitrary HTTPS URL.
3. A new launcher appears in the host desktop menu/app grid with a reasonable icon.
4. Launcher opens the site in a dedicated window (shell mode).
5. Web app storage is isolated per created app.
6. Notifications permission prompting works:
   - Ask/Allow/Block per origin
   - Persisted across launches
7. Allowed notifications appear as system notifications via portal.

---

## 16. Future Enhancements (Post-v1)

- Web Push support when app is closed (service worker background integration)
- More permission types with richer UI (site settings page)
- Import from existing browser profiles/bookmarks
- Theming options per web app (custom accent)
- Multi-window support and window-state persistence per app

---

## Appendix A: Directory Layout (Inside Flatpak Sandbox)

- App registry:
  - `$XDG_CONFIG_HOME/sitewrap/apps/`
- Permission store:
  - `$XDG_CONFIG_HOME/sitewrap/permissions/`
- Icons cache (source + generated):
  - `$XDG_CACHE_HOME/sitewrap/icons/`
- Web profiles (CEF user data):
  - `$XDG_DATA_HOME/sitewrap/profiles/<web_app_id>/`

Host-visible artifacts are exported via portals (not directly written).

---
