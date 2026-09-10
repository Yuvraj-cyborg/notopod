# Releasing

Versions follow [Semantic Versioning](https://semver.org/). All crates
share one version, set once in the root `Cargo.toml`. Releases are cut
from `main`; day-to-day work is on `dev`.

## Steps

1. On `dev`, with CI green, bump the version in two places in the root
   `Cargo.toml`: `[workspace.package] version` and the `version = "..."`
   of each `notopad-*` entry under `[workspace.dependencies]`.
2. In `CHANGELOG.md`, move everything under **Unreleased** into a new
   `## [X.Y.Z] - YYYY-MM-DD` section and update the links at the bottom.
3. `cargo build` so `Cargo.lock` picks up the new version. Commit:
   `release: vX.Y.Z`.
4. Open a pull request `dev -> main`. Merge it with a merge commit (not
   squash), so `main` and `dev` keep a shared history.
5. Tag the merge commit on `main` and push the tag:

   ```
   git checkout main && git pull
   git tag -a vX.Y.Z -m "notopad vX.Y.Z"
   git push origin vX.Y.Z
   ```

6. The **Release** workflow runs on the tag. It checks that the tag is
   on `main` and matches `Cargo.toml`, creates the GitHub release with
   the matching `CHANGELOG.md` section as its notes, then builds and
   attaches binaries (with sha256 sums) for:
   `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
   `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.
7. Merge `main` back into `dev` so the release commit is on both:
   `git checkout dev && git merge main && git push`.

## Publishing to crates.io

Optional and separate from the GitHub release. Run the **Publish crates**
workflow by hand (Actions tab, needs the `CARGO_REGISTRY_TOKEN` secret),
or locally from the tagged commit, in dependency order:

```
cargo publish -p notopad-core
cargo publish -p notopad-render
cargo publish -p notopad-editor
cargo publish -p notopad-tui
cargo publish -p notopad
```

`cargo publish` waits for each crate to appear in the index before the
next one is needed.

## Pre-releases

Tags like `v0.2.0-rc.1` go through the same workflow. The changelog
section must exist with exactly that version string.
