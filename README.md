# SpaceMaster

A disk cleaner for macOS. Tauri 2 + React, macOS 13 or later. Built for the host architecture; no
universal binary is produced.

It reclaims **system-level** space only. Caches under `~/Library` and the dotfile directories of
package managers are in scope; your own project directories are not — `node_modules`, `target` and
`build` belong to the tools that created them.

![The one-click clean page after a scan: the sidebar carries the nav and the free space on
the data volume, while each cache is one row with its size, file count and when it was last
touched. The notice across the top is the app explaining that the Trash cannot be measured
without Full Disk Access — which is why that row reads "Size unknown" rather than
zero.](docs/screenshot.png)

## Two entry points

**One-click clean.** A short, hand-written list of caches whose owning tool rebuilds them by
itself: npm's cacache, Homebrew's downloads, pip's wheels, CocoaPods, Xcode's own application
cache, application logs, crash report history, the Trash. Losing one of these costs a download,
never your work — so these are the only things the app ever deletes permanently.

**Professional mode.** Everything that needs a decision, and everything here goes to the Trash
rather than being deleted:

- **Developer caches** — the Cargo registry, Gradle, Maven, the Go module cache, SwiftPM, and
  whatever happens to live in `~/.cache`, listed one row each with its size and when it was last
  touched. Nothing here holds your work either, but rebuilding one costs real time.
- **Xcode** — `iOS DeviceSupport` per version, `DerivedData` per project, and archives (flagged,
  because deleting one means crash reports from that build can no longer be symbolized).
- **Simulators** — read from `xcrun simctl` rather than the filesystem, and deleted through it too,
  so the device set registry stays consistent. Sorted by when each device was last booted.
- **Leftovers** — caches, settings and containers named after software that is no longer installed.

Some caches have a safer official command than deleting the directory. Those are shown with the
command to run and **no delete button** — pnpm's store, for instance, is hard-linked into the
`node_modules` of every project on the machine, so removing it frees almost nothing and breaks
checked-out projects. `pnpm store prune` is the right answer, and it is not ours to run for you.

## How it tries not to delete the wrong thing

This is a program that deletes files you cannot get back, so most of the design is here rather than
in the feature list.

- A path becomes deletable only by passing sixteen checks (absolute, no `..`, not a symlink,
  `canonicalize()` identical to the input, strictly inside an allowed root **compared by path
  component** so `Caches-backup` is refused, not an ancestor of anything on the deny list, on the
  same volume as `$HOME`, not inside a running app's container, and so on). The type that
  represents "safe to delete" has private fields and one constructor, so there is no way to express
  a deletion that skipped them.
- Permanent deletion is restricted to the one-click table at compile time. Everything else can only
  reach the Trash, which means a wrong guess costs you a trip to the Trash rather than the file.
- Documents, Desktop, Downloads, Keychains, Mail, Messages, Photos, iCloud Drive,
  `Library/CloudStorage` and the iOS backup directory are refused outright — deleting inside a
  cloud mount would delete the cloud copy too.
- The interface never holds a file path it could ask to have deleted. It holds ids; paths travel
  outward only. There is no folder picker anywhere in the app.
- Every path is checked twice: once when the plan is shown to you, and again immediately before the
  deletion. Anything that changed on disk in between is refused and reported.
- Sizes are measured on disk (`st_blocks`), hard links are counted once, and the space actually
  freed is measured against the volume rather than added up from the files. Where the two numbers
  differ — clone-heavy caches make them differ — the interface says so instead of picking the
  flattering one.
- Every deletion is written to a log as it happens, so a run interrupted halfway can still tell you
  what it took. The History tab reads it back. There is no restore button: items in the Trash come
  back with Finder's "Put Back", and a permanent delete is gone — a button that claimed otherwise
  would be a lie.

Available in English and 简体中文, following the system language.

## Permissions

Nothing in the app requires Full Disk Access, and it is not sandboxed (a sandboxed app cannot read
another app's `~/Library`, which is most of what this one does). Two things do not work without the
grant: measuring the Trash, and moving an app container to the Trash. The app detects the state and
offers a button to System Settings rather than failing partway through.

Under `npm run tauri dev` the permission belongs to your **terminal**, not to SpaceMaster, so test
permission behaviour on the built app.

## Building

```bash
npm install
npm run tauri dev            # development
npm run tauri build          # .app and .dmg in src-tauri/target/release/bundle/
```

Checks:

```bash
npm run build                # type-check the frontend
npm run lint
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
```

Tests that read or write the real home directory are `#[ignore]`d; `cargo test -- --ignored` runs
them. Among them are acceptance harnesses (`src-tauri/tests/review_*.rs`) that print what a scan
decided, for checking a plan by eye before trusting it.

The build is ad-hoc signed (`signingIdentity: "-"`), which is a real signature with the hardened
runtime and sealed resources — but not a Developer ID one, and not notarized. Two consequences.
Gatekeeper refuses the first launch on any machine that did not build it, so a recipient has to
allow it once under System Settings ▸ Privacy & Security, or strip the quarantine flag by hand
(`xattr -dr com.apple.quarantine /Applications/SpaceMaster.app`). And an ad-hoc signature's hash
changes with every build, so the Full Disk Access grant is bound to that build alone and has to be
granted again after each one — including for whoever you hand a new version to. Signing with a
Developer ID and notarizing removes both problems; `providerShortName` and `entitlements` are left
in `tauri.conf.json` for whenever the yearly fee is worth it.
