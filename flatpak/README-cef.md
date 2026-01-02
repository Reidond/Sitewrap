# CEF Integration Notes (placeholder)

Sitewrap expects Chromium Embedded Framework assets to be present at runtime. Until the Flatpak manifest is fully wired, set these environment variables for local testing:

```
SITEWRAP_CEF_ROOT=/app/lib/cef
CEF_ROOT=/app/lib/cef
```

At runtime we expect:

- `/app/lib/cef/libcef.so`
- `/app/lib/cef/icudtl.dat`
- `/app/lib/cef/snapshot_blob.bin`
- `/app/lib/cef/v8_context_snapshot.bin`
- `/app/lib/cef/swiftshader/{libEGL.so,libGLESv2.so}`
- `/app/share/cef/locales/*.pak`
- `/app/share/resources/{resources.pak,chrome_100_percent.pak}`

The Flatpak manifest should eventually:

- Add a `modules` entry to unpack a CEF binary tarball into these paths.
- Export env vars:
- `SITEWRAP_CEF_ROOT=/app/lib/cef`
- `CEF_ROOT=/app/lib/cef`
- `LD_LIBRARY_PATH=/app/lib/cef:/app/lib`
- Ensure `--no-sandbox` is passed to CEF (Flatpak provides sandboxing).

The engine currently detects presence of `libcef.so` under `SITEWRAP_CEF_ROOT`/`CEF_ROOT` and will fall back to the stub view if assets are missing.

TODO for real CEF wiring:
- Add a Flatpak module to unpack an official CEF bundle into the paths above and enable the Cargo `cef` feature during the build of `sitewrap-engine`.
- In the `cef` backend, call `cef_do_message_loop_work` from `engine::tick()` and create an OSR view that forwards navigation/permission callbacks into the existing shell handlers.
- Pass `--no-sandbox` (or equivalent CEF setting) when spawning CEF subprocesses; rely on Flatpak sandbox for isolation.
- If using `extra-data`, ensure checksums are pinned and set `--env=LD_LIBRARY_PATH=/app/lib/cef:/app/lib` at runtime so libcef resolves correctly.
