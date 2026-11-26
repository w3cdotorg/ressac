# Release: consistent file names, and an RPM of the KDE app

Short design (26/11/2026). Clément asked for both after Ressac 0.3.0, the first release built by `release.yml`.

## Why

The 0.3.0 release mixes `Ressac-…` (the Mac app) and `ressac-…` (the terminal binaries), and nothing in a file name says which is the app and which is the terminal tool. It also has no Linux graphical app: `ressac-kde` installs only from a clone, with `apps/linux/install.sh`, which compiles it on the machine.

## Goals

1. The release files follow one pattern, from the next version on:

   | Contents | File |
   |---|---|
   | Mac app | `Ressac-<v>-macos-arm64.zip` (unchanged) |
   | Terminal app, Mac | `Ressac-cli-<v>-macos-arm64.tar.gz` |
   | Terminal app, Linux | `Ressac-cli-<v>-linux-x86_64.tar.gz`, `Ressac-cli-<v>-linux-aarch64.tar.gz` |
   | KDE app (and the terminal app), Fedora | `Ressac-<v>-linux-x86_64.rpm`, `Ressac-<v>-linux-aarch64.rpm` |
   | Checksums | `SHA256SUMS` (unchanged, now covering the RPMs) |

   The commands stay `ressac` and `ressac-kde`; the archives keep their single `ressac` binary at the top.
2. `sudo dnf install ./Ressac-<v>-linux-<arch>.rpm` on Fedora KDE installs a working app: Plasma launcher entry, icon, `ressac-kde` and `ressac` in `/usr/bin`, with dnf pulling Qt, Kirigami, the Breeze controls and QtWebEngine.
3. Uninstalling with `sudo dnf remove ressac` removes exactly those files. The database and settings in the home folder are never touched.

## Non-goals

- 0.3.0 stays as published (no re-upload, no renamed assets).
- No COPR repository, no Flatpak, no signed RPM (a later step if wanted).
- No distribution other than Fedora (the RPM is built on Fedora 44).
- No change to `apps/linux/install.sh`: it stays the way to install from a clone into `~/.local`.

## Anti-goals

- An RPM that installs but whose app fails at launch because a QML module (Kirigami, the Breeze controls) is missing: those are not library dependencies rpm can see, so they must be declared.
- A release that publishes without its RPMs: `publish` must fail if one is missing, as it does today for the archives.

## Approaches (chosen in conversation)

1. **An RPM attached to the release**, built in `fedora:44` containers on the GitHub runners (x86_64 on `ubuntu-latest`, aarch64 on `ubuntu-22.04-arm`, already proven by 0.3.0). **Chosen:** natural on Fedora, no new service, dnf resolves the dependencies.
2. COPR (the same RPM in a Fedora repository, updates through `dnf upgrade`): a later step on top of 1.
3. Flatpak with the KDE runtime: works everywhere, but the largest job (Rust and cxx-qt inside flatpak-builder, WebEngine). Only if Ressac has to leave Fedora.

## Decided

- **One RPM per architecture, built on Fedora 44.** The app uses only Qt's public API (no private headers, checked in `apps/linux/build.rs` and `apps/linux/cpp/`), so a build against Qt 6.11 runs on any Fedora whose Qt is 6.11 or newer: Fedora 44 and 45 today. If a later Fedora breaks it, the file name gains the Fedora version (`…-fc44-…`), not before.
- **The RPM package is named `ressac`** and carries both binaries, since the app and the terminal share `~/.local/share/ressac/ressac.sqlite`.
- **Licence field:** `MIT AND CC0-1.0` (the workspace is MIT in `Cargo.toml`; Bedstead, compiled into `ressac-kde`, is CC0).
- **The binaries are built with `cargo build --release --locked`** in the release job, then packaged by `rpmbuild` from a spec file in the repository. The RPM does not rebuild from source.
- **The release job checks the RPM** in its own container before uploading it: `dnf install` of the file, `ressac --version`, no library reported missing for `/usr/bin/ressac-kde`, `desktop-file-validate` on the entry.
- **The final check is on marghe:** before the next tag, the RPM built locally in the dev container (aarch64) is installed on the VM (Fedora 45) and launched from the Plasma launcher. The `dnf install` on the VM needs Clément's go-ahead, as always.

## Left to the builder

- Where the spec file lives (`apps/linux/packaging/ressac.spec` is the obvious place) and how the version reaches it (from the tag, through `rpmbuild --define`).
- The exact `Requires:` list for the QML modules and WebEngine (from `apps/linux/dev/packages.txt`, keeping only what runs, not what builds), and whether `breeze-icon-theme` is `Requires` or `Recommends`.
- The static desktop entry for `/usr/share/applications` (same keys as the one `install.sh` generates, `Exec=ressac-kde`), and the icon under `/usr/share/icons/hicolor/scalable/apps/`.
- Whether bundled fonts other than Bedstead ship in the KDE app (if so, their licences join the licence field and `%license`).
- The README: the "Releases" paragraph (new names) and the Linux app section (installing from the RPM, before the from-a-clone route).
- The next version number: the PR does not bump it; tagging stays Clément's call.
