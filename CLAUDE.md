# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

SpaceMaster: a macOS disk cleaner (Tauri 2 + React 19 + TypeScript + Vite). Two entry points —
one-click cleaning of caches their owning tool rebuilds (deleted permanently), and a professional
mode covering developer caches, Xcode/simulator junk, and leftovers of uninstalled apps (always to
the Trash).

`README.md` is still the unmodified Vite template and describes nothing about this app. Ignore it.

## Commands

```bash
npm run tauri dev            # run the app (see "Running the app" below — do not launch this yourself)
npm run build                # tsc -b && vite build — the ONLY frontend type-check
npm run lint                 # oxlint; silent on success
npm run tauri build          # .app + .dmg into src-tauri/target/release/bundle/

cd src-tauri
cargo test                   # unit + integration, excluding the real-machine ones
cargo test guard_refuses     # single test or module by substring
cargo clippy --all-targets -- -D warnings
```

`npx tsc --noEmit` does **not** cover `src/` — the project references live in `tsconfig.app.json`,
so `npm run build` is the only way to type-check the frontend.

### Real-machine tests

Anything touching the real home directory is `#[ignore]`d and must be asked for:

```bash
cargo test --lib safety -- --ignored          # asserts real sensitive paths are all refused
cargo test --test measure_vs_du -- --ignored  # size accounting vs `du`
cargo test --test review_orphans -- --ignored --nocapture
```

The `tests/review_*.rs` files are not assertions — they are human acceptance harnesses that print
what the pipeline decided (and `open -R` commands to check rows one by one). Each file's header
comment says what to look for. `tests/trash_round_trip.rs` actually writes to `~/.Trash`.

## The safety architecture

This app deletes files a user cannot get back, so the Guard outranks any UI concern. Seven
independent layers, each of which still holds if another fails:

1. **`safety/guard.rs::vet()` is the only constructor of `SafeTarget`**, whose fields are private.
   Everything in `remove/` takes `&SafeTarget`. There is no way to express "delete this path"
   without going through the Guard.
2. **Rules R1–R16** as `RuleId` variants. Load-bearing ones that look redundant but are not:
   R5 requires `canonicalize()` to equal the input verbatim (symlink escapes); R6 compares by path
   *component* so `Caches-backup` is refused; R9 refuses any **ancestor of a deny entry**, so
   `~/Library` cannot be deleted on the way to Keychains; R12 requires the same `st_dev` as `$HOME`
   (external, network, and separately-mounted cloud volumes).
3. **R15 (`PermanentNotAllowed`) confines `DeleteMode::Permanent` to `catalog/quick.rs`.** That
   table is the single source of truth for permanent deletion; the Guard reads it directly. Note
   `~/Library/Logs` is a Tier A entry, so R15 legitimately permits Permanent beneath it.
4. **`safety/deny.rs`** — largest blast radius in the repo. Includes `Library/CloudStorage`,
   `Library/Mobile Documents`, and `Application Support/CloudDocs`, because deleting inside a cloud
   mount deletes the cloud copy too.
5. **`src-tauri/clippy.toml`** makes `fs::remove_*` a compile error outside `remove/`;
   `remove/permanent.rs` holds the single `#[allow]`.
6. **No command takes a path as input.** The frontend holds ids, not paths; paths travel outward
   only. Deliberate exceptions: `delete_simulators(udids)` and `reveal_orphan(generation, id, place)`.
7. `tests/review_quick_plan.rs` etc. exist so a plan can be reviewed by a human before it runs.

When adding a scan target, the question is which catalog table it belongs to: `catalog/quick.rs`
(Tier A — rebuilt automatically, no side effect, permanent delete) or `catalog/dev.rs` (Tier B —
rebuilding costs real time, never pre-selected, Trash only). Caches with a safer official command
go in `ADVISORIES` instead: a path plus the command, **no delete button** (see `pnpmStore`).

## Three-stage IPC

```
run_*_scan()                              -> ScanReport { generation, items[] }   (ids, no paths)
preview_clean(generation, itemIds, mode)  -> CleanPlan { token, accepted[], rejected[] }
execute_clean(token)                      -> CleanOutcome
```

`generation` invalidates stale scan results; `token` is single-use. Every path is vetted twice —
at preview and again immediately before deleting — so `CleanOutcome` has a `rejected` list of its
own for things that changed on disk in between.

## The i18n contract

**Rust never returns prose for display.** It returns stable machine-readable discriminants
(`AppError::kind()`, `RuleId`, `ScanIssue` kinds, catalog ids) and the frontend owns all wording.
The one exception is `FailedRecord::detail`, macOS's own error string, shown verbatim because
inventing wording for it would mean guessing what the OS meant.

`src/i18n/locales/en.ts` is the base locale and `LocaleResource` type-checks every other locale
against it, so a missing or misspelled key is a compile error rather than a runtime fallback. It
allows exactly two levels of nesting. `catalog.<id>.*` keys mirror the Rust catalog tables; an
unrecognised id falls back to printing the raw id (deliberate for rows discovered at scan time,
a bug for rows we wrote down). Never hardcode Chinese, and never hardcode display English outside
the locale files.

## Scan engine

`fsutil/walk.rs` uses jwalk. Four silent-failure sources, each of which produces a wrong-but-
plausible small number rather than an error, and each with a regression test:

- `.skip_hidden(false)` — jwalk defaults to `true`, which would silently miss `~/.npm`, `~/.cargo`.
- `st_dev` compared per directory in `process_read_dir` — jwalk has no `same_file_system` option.
- hard links deduped by `(dev, ino)`, recorded only when `nlink > 1`.
- cancellation checked per **entry**, not per directory.

Sizes are `st_blocks * 512` (on-disk), not `len()`, because APFS has sparse files and clones.
Directory `st_blocks` is 0 on APFS and skipped. A permission error records a `ScanIssue` and never
returns a bare zero — "已经是空的" for an unreadable multi-gigabyte directory is the failure mode
this rule exists to prevent.

Scans run on a dedicated rayon pool via `spawn_blocking` (never on a tokio worker). Progress is
throttled to ≥80 ms **or** ≥256 MB, and `GroupReady` pushes each group as it finishes.

Reported size and freed size are separate numbers (`statfs` before/after) and the UI explains the
gap rather than pretending to precision — clone-heavy caches like bun's differ visibly.

## Deletion and the ledger

`trash` crate with `DeleteMethod::NsFileManager` set explicitly — the default is `Finder`, which
drives osascript and needs Apple Events permission that an ad-hoc-signed app must re-grant every
rebuild. A test asserts the method to stop a refactor reverting it.

Finder's "Put Back" **does** work on items trashed this way (verified on macOS 26.4; the crate's
own docs claim otherwise and are outdated). So `remove/ledger.rs` is an **audit log, not an undo
stack**: there is no `undo_batch`, and nothing tries to locate trashed items programmatically
(impossible anyway — `~/.Trash` cannot be listed without Full Disk Access). `ledger::history()` is
the sole parser of the file, and `unfinished()` is a filter over it, so "what did this app delete"
cannot have two disagreeing answers.

Permanent deletion is `std::fs::remove_dir_all` only — never a hand-written recursion. Records are
flushed per entry, release keeps `panic = "unwind"`, and each entry is wrapped in `catch_unwind`.

## macOS permissions

Capabilities only gate **IPC**, not Rust. `capabilities/default.json` therefore holds just
`["core:default"]` — no `fs:*`, no `shell:*`. Reveal-in-Finder and Open-Settings are Rust commands.

Full Disk Access is probed by opening `~/Library/Application Support/com.apple.TCC/TCC.db`
(`safety/privacy.rs`), which succeeds if and only if the grant is held. **In `tauri dev` the grant
belongs to the terminal, not to SpaceMaster**, so a dev-mode permission test says nothing about the
bundled app. Two things genuinely need it: listing `~/.Trash`, and moving `~/Library/Containers/<id>`
to the Trash (that refusal arrives as `Error::Unknown` with no error code, so it is inferred from
`Error::Unknown` + `!full_disk_access()`).

The bundle is ad-hoc/linker-signed, so an FDA grant is bound to a cdhash that changes on every
`tauri build` and must be re-granted after each one.

`Info.plist` deliberately has **no** `NS*FolderUsageDescription` keys; the comment there explains
why, and it should be read before adding any.

## Scope boundary

The app cleans **system-level space only** and never reaches into the user's own project
directories. `node_modules`, `target`, `build`, `dist` are out of scope. Consequently the Guard has
no notion of a user-picked deletion root and the app offers no folder picker — adding either
reopens a settled decision.

## Conventions

- **All code comments and doc comments in English**, including config files, even though the
  working conversation may be in Chinese.
- Comments here carry the *why* — a rule that looks redundant, a default that had to be overridden,
  a decision that was made against the obvious alternative. Keep that bar; do not add comments
  restating what the code says.

## Running the app

Do not start `npm run tauri dev` from a background shell — detached it exits immediately with code
0. Ask the user to run it in their own session (`! npm run tauri dev`), say what to click, and
verify afterwards from the ledger and from `df`.
