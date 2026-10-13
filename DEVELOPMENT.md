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
  binaries as artifacts.
- Not covered by CI: the hidden `set-token` prompt. Try it by hand.

## Release

1. Bump `version` in `Cargo.toml` and `v=` in the README. Commit, push, and
   wait for CI.
2. `git tag -a vX.Y.Z -m "git-credential-ghpat X.Y.Z" && git push origin vX.Y.Z`
3. From that commit's CI artifacts, make
   `git-credential-ghpat-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` and
   `git-credential-ghpat-vX.Y.Z-x86_64-pc-windows-msvc.zip` (each with
   `README.md` and `LICENSE`), plus `SHA256SUMS`.
4. `gh release create vX.Y.Z --verify-tag --title vX.Y.Z git-credential-ghpat-vX.Y.Z-* SHA256SUMS`
