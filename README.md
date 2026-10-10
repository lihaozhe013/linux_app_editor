# Linux App Editor

A small Linux desktop-entry creator/editor for manually installed applications.

It is not a package manager or application manager. It does not watch your system, detect installed
packages, or manage launchers created by other software. Its launcher tools create and edit
`.desktop` files and scan only fixed, well-known desktop-entry directories on demand. The Systemd
Services page separately browses known systemd unit paths.

The app also includes a **Systemd Services** editor for `.service` units and a separate SSH-friendly
TUI, `systemd-service-editor`. The GUI and TUI share one Rust editing core. The TUI has no GTK,
WebKit, or Tauri dependency and runs in a headless SSH session.

## Why

You downloaded, unpacked, built, or dropped a binary somewhere:

```text
~/dev/foo/dist/release/foo
        ↓  Linux App Editor
foo.desktop  (~/.local/share/applications)
        ↓
GNOME / KDE / Cinnamon / Xfce / rofi drun launcher
```

## What it does

- **Create Launcher** — pick an executable, fill in Name / Arguments / Icon / Working Directory /
  Terminal / Comment / Categories, get a spec-compliant `.desktop` file in
  `$XDG_DATA_HOME/applications` (default `~/.local/share/applications`). Executables can be dragged
  onto the window; the file name is used as a Name suggestion you are free to edit.
- **AppImage support** — when an executable ends in `.AppImage`, its icon and desktop-entry details
  (Name, Comment, Categories) are read straight out of the embedded SquashFS payload — the AppImage
  itself is never executed and is not extracted as a whole. The icon is copied to
  `~/.local/share/linux-app-editor/icons` so the launcher keeps working when the AppImage is moved
  or replaced by an updated download, and empty form fields are pre-filled from the embedded
  metadata.
- **Managed Launchers** — lists the files this tool created (marked with
  `X-LauncherEditor-Managed=true`). Only that directory is read, only non-recursively, and only for
  files carrying the marker.
- **Open .desktop** — browse and fuzzy-search well-known desktop entry directories (XDG user and
  system `applications`, autostart, Flatpak, Snap, Nix profiles, `/opt`). Open any listed entry or
  file through the dialog, then Save or Save As. Delete listed entries after confirmation, or
  display safely quoted `rm -f --` and `sudo rm -f --` commands to copy and run manually. The sudo
  option is available regardless of whether deletion has failed; use it only if needed, authorized,
  and certain the file should be removed. Commands are never run by the app. Deletion removes only
  the `.desktop` file, not its application or backup, and a package manager may restore
  package-provided entries. Unknown keys, `X-*` keys, locale keys (`Name[zh_CN]`, …),
  `Desktop Action` sections and comments are preserved on save.
- **Systemd Services** — browse, create, validate, and edit user or system `.service` files. Common
  directives have structured fields; raw unit text handles advanced settings. Review the diff and
  validation before saving. Editing a vendor unit creates a local drop-in and leaves the package
  file untouched. The GUI stages system-scope files and shows the exact `sudo install` command; it
  does not request privilege elevation itself.
- **Service Templates** — create a service from a template by entering one program path. Two
  templates ship today, both producing `Type=simple` with `Restart=always`, `RestartSec=5s` and
  start-rate limits: _Autostart program (user)_ installs into `$XDG_CONFIG_HOME/systemd/user` with
  `WantedBy=default.target`, and _Autostart program (system, sudo)_ installs into
  `/etc/systemd/system` with `WantedBy=multi-user.target`. The unit name is suggested from the file
  name and stays editable. The path must be absolute, existing, and executable, because systemd
  refuses such a unit at start time. A blank unit stays available for everything else.
- **Systemd Service TUI** — launch `systemd-service-editor` from a terminal or SSH session. It
  offers the same browse, create, form, raw edit, validation, save, and reload workflows. Press `p`
  in the service list to open an existing `.service` file by absolute path; files outside recognized
  systemd unit paths are edited in place. Press `t` to create a service from the template matching
  the active scope: it asks for a program path, then a unit name. Scope defaults to user. Run
  `sudo systemd-service-editor --scope system` for direct machine-wide edits. Under `sudo`, user
  scope uses the sudo process environment, which may point to root's user-unit paths. Reloading unit
  files does not start, stop, or restart a service.

Arguments are real desktop-entry arguments, not a shell command; the Exec value is quoted and
escaped per the specification. Field codes such as `%f`, `%U` or `%%` survive a round trip. An
optional `desktop-file-validate` check runs when the binary is on `$PATH` — it is not a dependency.

## Risk model

The desktop-entry editor intentionally allows editing arbitrary `.desktop` files. Files managed by
package managers or other software may be overwritten later. The user is responsible for changes to
externally managed files. When a file lives in a location like `/usr/share/applications` the editor
shows a non-blocking notice and lets you continue; if writing fails due to permissions, the error is
shown as-is. The desktop app does not invoke sudo, polkit, or any other privilege escalation.

## Versions

| Component        | Version | Notes                                                                 |
| ---------------- | ------- | --------------------------------------------------------------------- |
| Tauri            | 2.12    | Rust backend, `tauri-plugin-dialog` for native file dialogs           |
| Rust             | 2024 ed | Built and tested with 1.98                                            |
| Svelte           | 5.5x    | Runes, no external state management                                   |
| Vite             | 8.3     | ESNext target, no legacy transpilation                                |
| Bun              | 1.4     | Sole package manager / script runner (no npm, pnpm, yarn)             |
| TypeScript (tsc) | 7.0.2   | Native compiler, per project requirements                             |
| Ratatui          | 0.30.2  | TUI renderer; standalone `systemd-service-editor` binary              |
| typescript (pkg) | 6.0     | The `typescript` package name is aliased to `@typescript/typescript6` |

TypeScript note: `tsc` is the 7.0 native compiler. TS 7 ships no stable programmatic API yet, so
tooling that embeds the compiler (svelte-check, typescript-eslint) cannot use it. Following the
official side-by-side recommendation from the TypeScript team, the `typescript` package is aliased
to `@typescript/typescript6` (used only by ESLint's parser), while `tsc` remains 7.0.2 for type
checking. As a consequence Svelte components are not type-checked by svelte-check; all non-trivial
frontend logic lives in plain `.ts` modules which `tsc` fully covers.

Icon preview reads arbitrary absolute paths through Tauri's asset protocol (scope `**`). This is a
local, read-only image fetch — the documented trade-off for not restricting which icons can be
previewed.

## Development

```sh
bun install          # frontend dependencies
bun x tauri dev      # run the app in dev mode
bun x tauri build    # build the release bundle (deb)
```

Checks:

```sh
bun run check        # tsc --noEmit (TS 7 native)
bun run lint         # eslint (.ts, .js, .svelte)
bun run fmt          # prettier
bun run test         # bun test (frontend units)
bun x vite build     # frontend production build

cd src-tauri
cargo test --workspace           # desktop app, shared core, and TUI
cargo clippy --workspace --all-targets
cargo fmt --all
```

## Nightly builds

Pushing to the `publish` branch runs the release workflow (`.github/workflows/release.yml`): quality
gates first, then native builds on `ubuntu-24.04` (x86_64) and `ubuntu-24.04-arm` (aarch64). Eight
assets are published to the rolling `nightly` release as a draft and only published after the full
asset set is verified and the branch has not moved:

```text
linux-app-editor-linux-{x86_64,aarch64}.AppImage   self-contained
linux-app-editor-linux-{x86_64,aarch64}.tar.gz     bare binary; needs GTK 3 + WebKitGTK 4.1
systemd-service-editor-linux-{x86_64,aarch64}.tar.gz  headless TUI; no desktop libraries
linux-app-editor-linux-{x86_64,aarch64}.deb        Debian package (amd64/arm64)
```

The arm64 job requires the free GitHub-hosted arm runners (public repositories).

## Layout

```text
src/                  Svelte 5 frontend (views in src/lib/components)
src-tauri/src/desktop_entry/   parser / serializer / exec / fields / validation
src-tauri/src/appimage.rs      read-only AppImage icon/metadata extraction
src-tauri/src/filesystem.rs    XDG resolution, atomic writes
src-tauri/src/icons.rs         bounded "find nearby icons" scan
src-tauri/src/locations.rs     well-known desktop entry directories (read-only)
src-tauri/src/commands.rs      the narrow Tauri command API
src-tauri/crates/systemd-service-core/  shared systemd unit parsing, validation, and file operations
src-tauri/crates/systemd-service-core/src/templates.rs  unit templates for create-from-template
src-tauri/crates/systemd-service-tui/   SSH-friendly Ratatui executable
src-tauri/tests/      round-trip fixtures and command integration tests
```
