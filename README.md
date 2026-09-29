# Linux App Editor

A small Linux desktop-entry creator/editor for manually installed applications.

It is not a package manager or application manager. It does not watch your system, detect installed
packages, or manage launchers created by other software. It does one job: create and edit `.desktop`
files. The only thing it reads beyond files you open is a fixed list of well-known desktop entry
directories, on demand, non-recursively and capped.

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
- **Managed Launchers** — lists the files this tool created (marked with
  `X-LauncherEditor-Managed=true`). Only that directory is read, only non-recursively, and only for
  files carrying the marker.
- **Open .desktop** — a read-only browser of every well-known desktop entry directory (XDG user and
  system `applications`, autostart, Flatpak, Snap, Nix profiles, `/opt`), with fuzzy search over
  name, filename and path. Open any entry from the list, or any file through the dialog, then Save
  or Save As. Unknown keys, `X-*` keys, locale keys (`Name[zh_CN]`, …), `Desktop Action` sections
  and comments are preserved on save.

Arguments are real desktop-entry arguments, not a shell command; the Exec value is quoted and
escaped per the specification. Field codes such as `%f`, `%U` or `%%` survive a round trip. An
optional `desktop-file-validate` check runs when the binary is on `$PATH` — it is not a dependency.

## Risk model

The application intentionally allows editing arbitrary `.desktop` files. Files managed by package
managers or other software may be overwritten later. The user is responsible for changes to
externally managed files. When a file lives in a location like `/usr/share/applications` the editor
shows a non-blocking notice and lets you continue; if writing fails due to permissions, the error is
shown as-is. There is no sudo, polkit, or any kind of privilege escalation.

## Versions

| Component        | Version | Notes                                                                 |
| ---------------- | ------- | --------------------------------------------------------------------- |
| Tauri            | 2.12    | Rust backend, `tauri-plugin-dialog` for native file dialogs           |
| Rust             | 2024 ed | Built and tested with 1.98                                            |
| Svelte           | 5.5x    | Runes, no external state management                                   |
| Vite             | 8.3     | ESNext target, no legacy transpilation                                |
| Bun              | 1.4     | Sole package manager / script runner (no npm, pnpm, yarn)             |
| TypeScript (tsc) | 7.0.2   | Native compiler, per project requirements                             |
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
cargo test           # domain, filesystem and command tests
cargo clippy --all-targets
cargo fmt
```

## Nightly builds

Pushing to the `publish` branch runs the release workflow (`.github/workflows/release.yml`): quality
gates first, then native builds on `ubuntu-24.04` (x86_64) and `ubuntu-24.04-arm` (aarch64). The six
assets are published to the rolling `nightly` release as a draft and only published after the full
asset set is verified and the branch has not moved:

```text
linux-app-editor-linux-{x86_64,aarch64}.AppImage   self-contained
linux-app-editor-linux-{x86_64,aarch64}.tar.gz     bare binary; needs GTK 3 + WebKitGTK 4.1
linux-app-editor-linux-{x86_64,aarch64}.deb        Debian package (amd64/arm64)
```

The arm64 job requires the free GitHub-hosted arm runners (public repositories).

## Layout

```text
src/                  Svelte 5 frontend (views in src/lib/components)
src-tauri/src/desktop_entry/   parser / serializer / exec / fields / validation
src-tauri/src/filesystem.rs    XDG resolution, atomic writes
src-tauri/src/icons.rs         bounded "find nearby icons" scan
src-tauri/src/locations.rs     well-known desktop entry directories (read-only)
src-tauri/src/commands.rs      the narrow Tauri command API
src-tauri/tests/      round-trip fixtures and command integration tests
```
