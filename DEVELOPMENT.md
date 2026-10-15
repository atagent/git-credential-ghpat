# Development notes

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

- `src/main.rs`: CLI, credential helper protocol, file location and I/O.
- `src/table.rs`: credentials table logic, strings only.
- `tests/cli.rs`: runs the built binary.
- CI (`.github/workflows/ci.yml`) tests on Linux and Windows and uploads both
  binaries as artifacts. On a `v*` tag, its `release` job also creates a
  draft release from them.
- Not covered by CI: the hidden `set-token` prompt. Try it by hand.

## Release

1. Bump `version` in `Cargo.toml` and `v=` in the README. Commit, push, and
   wait for CI.
2. `git tag -a vX.Y.Z -m "git-credential-ghpat X.Y.Z" && git push origin vX.Y.Z`
3. CI builds and tests the tag, checks it matches `Cargo.toml`, and creates
   a draft release `vX.Y.Z` with
   `git-credential-ghpat-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` (holding
   only `git-credential-ghpat`),
   `git-credential-ghpat-vX.Y.Z-x86_64-pc-windows-msvc.zip` (holding only
   `git-credential-ghpat.exe`), and `SHA256SUMS` for the two archives. The
   notes list the downloads and link the CI run.
4. On GitHub, add what changed to the draft's notes and publish it.
