# Contributing

Thanks for helping. This page is short on purpose.

## Set up

You need Rust 1.88 or newer. `rust-toolchain.toml` picks the current
stable and adds `rustfmt` and `clippy`. With Nix, `nix develop` gives you
the same tools.

```
cargo build
cargo run -- /tmp/scratch.md
cargo install --path .      # puts `notopod` on your PATH for day-to-day use
```

## Before you push

CI runs exactly these, on Linux, macOS and Windows, with warnings treated
as errors:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

plus `nix build` on Linux. Clippy runs with the `pedantic` group on; the
few lints we turn off are listed in the root `Cargo.toml` with a reason.

## Branches

- `dev` is the default branch. Every pull request targets `dev`.
- `main` only receives releases, through a pull request from `dev`.
  Nobody pushes to `main` directly.

Branch from `dev`, name the branch after the change (`render/table-alignment`,
`editor/selection`), open a pull request against `dev`.

## Commits and pull requests

Short, imperative subject lines. Prefix with the crate when it helps:

```
render: box fenced code blocks
editor: group backspaces in undo
release: v0.2.0
```

If a user would notice the change, add a line under **Unreleased** in
`CHANGELOG.md` in the same pull request.

## Releases

Versions follow [Semantic Versioning](https://semver.org/): while we are
on `0.x`, a minor bump may break things, a patch bump may not. One
version for the whole workspace, set in `[workspace.package]` in the root
`Cargo.toml`; every crate inherits it.

1. On `dev`: bump the version, move the **Unreleased** notes in
   `CHANGELOG.md` under a new `## [X.Y.Z] - YYYY-MM-DD` heading,
   `cargo build` to refresh `Cargo.lock`, commit as `release: vX.Y.Z`.
2. Merge `dev` into `main` (merge commit, not squash).
3. Tag on `main` and push the tag:

   ```
   git checkout main && git pull
   git tag -a vX.Y.Z -m "notopod vX.Y.Z" && git push origin vX.Y.Z
   ```

The **Release** workflow checks that the tag matches `Cargo.toml`, makes
the GitHub release with that changelog section as its notes, and attaches
binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64) and
Windows (x86_64). Merge `main` back into `dev` afterwards.

Nothing goes to crates.io: the library crates have short unprefixed names
that are taken or too generic for the registry, so they are
`publish = false`, and a binary cannot be published with unpublished path
dependencies. Users get releases, Nix, or `cargo install --git`.

## Where things live

| Path | Contents |
|---|---|
| `src/` | The `notopod` command-line entry point (root package) |
| `lib/syntax` | Document model, Markdown parser, line index |
| `lib/render` | Theme, block and inline rendering, wrapping, ANSI output |
| `lib/editor` | Rope buffer, cursor, undo history, list-aware Enter, search, save |
| `lib/tui` | App state, key handling, screen layout, drawing |
| `docs/` | Architecture notes and the release procedure |

New library crates go in `lib/<name>` with a short name and
`publish = false`, and get one line in the tables here and in the README.

`docs/ARCHITECTURE.md` explains how the pieces fit and how to add a new
kind of block.

## Testing the screen

Layout logic (`lib/tui/src/view.rs`) has unit tests. For behaviour,
run the editor and try the awkward cases: a list with an empty item,
Enter inside a word, Ctrl+Z after pasting, a very long line, resizing the
terminal, a file that does not exist yet.
