# sitewrap-engine CEF detection (current state)

- The engine looks for CEF assets under `SITEWRAP_CEF_ROOT` or `CEF_ROOT` (first hit wins).
- Detection is a lightweight probe for `libcef.so`.
- Backends:
  - StubBackend: default when no CEF assets are found.
  - PlaceholderCefBackend: chosen when assets are present but bindings are not yet wired; renders the stub view while signaling CEF readiness.
  - A real CEF backend can replace the placeholder without changing callers (EngineBackend trait).
- `engine::tick()` calls a backend-provided hook every ~16ms; when CEF is integrated, put `cef_do_message_loop_work` there.
- `cef` Cargo feature is off by default; enable it when adding bindings.
- CEF backend stub (`cef_backend.rs`) is feature-gated; it currently returns a placeholder widget. Integrate real bindings (e.g., cef-sys) under the `cef` feature and set the tick hook to pump the CEF message loop. The runtime still checks for `libcef.so` under `SITEWRAP_CEF_ROOT`/`CEF_ROOT`.
- TODOs for CEF backend (feature `cef`):
  - dlopen `libcef.so` from `SITEWRAP_CEF_ROOT`/`CEF_ROOT`
  - initialize CEF with `windowless_rendering_enabled = true` and per-profile cache path
  - hook `engine::tick` to call `cef_do_message_loop_work`
  - create OSR browser, render to texture, forward GTK input, and invoke navigation callbacks

Expected runtime layout (matching Flatpak placeholders):

```
/app/lib/cef/libcef.so
/app/lib/cef/icudtl.dat
/app/lib/cef/snapshot_blob.bin
/app/lib/cef/v8_context_snapshot.bin
/app/lib/cef/swiftshader/{libEGL.so,libGLESv2.so}
/app/share/cef/locales/*.pak
/app/share/resources/{resources.pak,chrome_100_percent.pak}
```

Environment vars (set in the Flatpak manifest placeholders):

- `SITEWRAP_CEF_ROOT=/app/lib/cef`
- `CEF_ROOT=/app/lib/cef`
- `LD_LIBRARY_PATH=/app/lib/cef:/app/lib`
- `CEF_FORCE_SANDBOX=0` (CEF sandbox off; Flatpak provides isolation)

Next wiring steps (not yet implemented):

- Initialize CEF with `multi_threaded_message_loop = false` and pump `cef_do_message_loop_work` from the GTK main loop.
- Create an OSR browser surface and forward navigation/click/keyboard events from the GTK widget.
- Hook permission prompts and external navigation policy into the CEF client handlers.
