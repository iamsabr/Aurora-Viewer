# This fork

`iamsabr/Aurora-Viewer` is a personal fork of Aurora Viewer, built and used on
its own. It follows the official project's **released versions** only, with
its own features merged on top. Nothing from it is sent upstream: no commits,
pull requests, issues or crash reports.

## What the fork adds

- **"Accept in"** (from Catznip): the Open window and inventory offers can file
  what you accept into a chosen folder, the giving object's folder, Received
  Items, or saved destination folders with dated and region subfolders
  (`world/contents_destination.rs`, `world/folder_path.rs`,
  `accept_in_saved.rs`, `ui/accept_in_editor.rs`, `ui/accept_in_picker.rs`).

## What a fork build does differently

All of it is in `crates/aurora-viewer/src/fork.rs`, so that an upstream release
merges in with as few conflicts as possible.

- **No updates from upstream.** The updater only works in a build whose version
  the official Release workflow publishes (a stable version or a `beta`
  pre-release). A fork release is tagged `vX.Y.Z-fork.N`, which is neither, so
  a fork build never checks for, offers or installs an official release, and
  is never asked to update before logging in. Nothing to set.
- **Crash reports stay on this PC.** The crash window never files a report on
  the official issue tracker; it keeps "Open folder" and "Copy trace" to share
  a report by hand.

Settings, accounts and caches are shared with any official Aurora install on
the same PC: what Preferences changes in one applies to the other.

## Versions

A fork release is a tag on the fork: **`vX.Y.Z-fork.N`**.

- `X.Y.Z` is the upstream version line the fork is built on.
- `N` counts the fork's releases on that line, from 1.
- A build between two fork tags reads `X.Y.Z-fork.N+<commits>.g<commit>`.

Help › About and the log show this version. `v0.9.14-fork.1` is built on
upstream's 0.9.14 betas (up to 2026-11-24); the next upstream stable release
merged in starts the next number.

## Building the day-to-day viewer

```powershell
./scripts/build-release.ps1
```

It builds the fork's `main` as it is on GitHub (never local or unpushed
changes), with full optimisation, into
`..\RELEASE\Aurora-Viewer\aurora-viewer.exe`. The first run also creates
`..\RELEASE\build-release.bat`: double-click it to rebuild after a fork release.
Push the release tag before building, so the build carries its fork version.

## Pulling in an upstream release

Only upstream's published releases come in (`vX.Y.Z` for stable, or a
`vX.Y.Z-beta.<date>.<n>` when a beta is wanted), never its day-to-day `main`.

1. Fetch the release: `git fetch upstream --tags`, and pick the tag
   (`git tag --list "v*" --sort=-creatordate | Select-Object -First 10`).
2. Start a task from the fork's `main`:
   `./scripts/new-task.ps1 -Name "sync-vX.Y.Z" -Kind chore`.
3. In the task, merge the release tag: `git merge vX.Y.Z`. A merge (not a
   rebase) keeps the fork's history, and each conflict is solved once.
   Conflicts are expected in `TASKS.md` and in files the fork's features
   change; keep both sides. If upstream changed a file `fork.rs` relies on,
   check that `fork.rs` still does what this page says.
4. `./scripts/check.ps1`, then test the fork's features in the viewer
   (`cargo run -p aurora-viewer -- --title "Test sync-vX.Y.Z"`).
5. Push to the fork and merge the pull request on the fork
   (`gh pr create --repo iamsabr/Aurora-Viewer`).
6. Tag the fork release on the merged `main` and push the tag to the fork:
   `git tag -a vX.Y.Z-fork.1 origin/main -m "Aurora Viewer X.Y.Z, fork 1"`,
   then `git push origin vX.Y.Z-fork.1`.
7. Rebuild: `..\RELEASE\build-release.bat`.

## Keeping it on the fork

- `gh` defaults to the fork (`gh repo set-default iamsabr/Aurora-Viewer`), and
  every `gh` command still names `--repo iamsabr/Aurora-Viewer`.
- The `upstream` remote can fetch but not push: its push URL is set to
  `DISABLED-never-push-to-upstream`.
- The fork's GitHub Actions have never run (a fork's workflows stay off until
  they are enabled in its Actions tab). Leave them off: the Release workflow
  would otherwise start on each fork tag.
