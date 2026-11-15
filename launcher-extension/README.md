# Vicinae package manager

Open **Install apps**, search for an application, and open its actions. Installed
catalog apps offer **Uninstall**, with a confirmation before removal. After
removal, **Install** becomes available again. The command also matches the
keywords `uninstall`, `remove`, and `manage`.

Installed status is read from the system Flatpak installation, Pacman's local
database, and the current user's Nix profile each time a search is requested.
Only catalog packages are managed. Missing tools or failed status queries do not
enable uninstall. Nix matches the exact nixpkgs attribute to its profile entry
name; Flatpak uses the system installation and stable branch used by installation.

Removal does not request app-data deletion or dependency cleanup. Pacman can
refuse removal when another package depends on the app. Package-manager failures
are reported as a failure toast, leaving the action available to retry.

## Build

With Rust, a C/C++ toolchain, pkg-config, Flatpak development libraries, and Node
installed, run:

```sh
npm ci
npm run build
```

The `dist` directory contains the extension and its Rust bridge. Deploy both
together using your existing Vicinae extension installation process.

## Checks

```sh
npm run typecheck
cargo test --locked -p libpm
```
