# git-credential-ghpat

A git credential helper for using several GitHub PATs with one account.

Git's username for a request is used as an **identifier**. The helper looks
it up in a map file and answers with that PAT, sending `token` as the
username, so the identifier never reaches GitHub.

```
git → username=work → ghpat → username=token, password=github_pat_…
```

Works on Linux and Windows. It has no dependencies beyond the Rust standard
library.

## Install

```sh
cargo install --path .
```

This installs `git-credential-ghpat` (`git-credential-ghpat.exe` on Windows)
into `~/.cargo/bin`. That directory must be on your `PATH` so git can find the
helper.

## Map file

The file is looked up in this order:

| | Linux | Windows |
|---|---|---|
| 1 | `$GHPAT_FILE` | `%GHPAT_FILE%` |
| 2 | `$XDG_CONFIG_HOME/git-credential-ghpat/credentials` | same |
| 3 | `~/.config/git-credential-ghpat/credentials` | `%APPDATA%\git-credential-ghpat\credentials` |

`XDG_CONFIG_HOME` is usually unset, so step 3 is the common case. A relative
`XDG_CONFIG_HOME` is ignored, as the XDG spec requires.

```
# identifier  token
work          github_pat_AAAA...
side          github_pat_BBBB...
```

- Each line has two columns separated by whitespace. Blank lines are ignored.
- If an identifier is listed more than once, the **last** entry wins.
- Windows line endings (CRLF) and a UTF-8 BOM are accepted.

## Configure git

In `~/.gitconfig`:

```ini
[credential "https://github.com"]
    helper =
    helper = ghpat
    helper = !gh auth git-credential   # optional fallback
```

- `helper =` clears any helpers configured earlier, including the `manager`
  helper that the Git for Windows installer sets up.
- `ghpat` must come first. The fallback only answers requests that `ghpat`
  doesn't recognize.

## Choose a PAT

Set the identifier as the username. Any of these works:

```sh
# per repo
git config credential.https://github.com.username work

# by URL prefix, in ~/.gitconfig (also covers `git clone`)
git config --global credential.https://github.com/work-org.username work

# in the remote URL
git clone https://work@github.com/work-org/repo.git
```

Don't put a password in the URL (`https://work:TOKEN@…`). Git would use it
directly and never ask the helper.

## Test

```sh
printf 'protocol=https\nhost=github.com\nusername=work\n\n' | git credential fill
```

This prints the PAT, so only run it where nobody can see your screen. On
Windows, run it in Git Bash.

## Development

```sh
cargo test
```

## Behavior

| Request | Result |
|---|---|
| identifier is in the map | `username=token`, `password=<PAT>` |
| identifier not in the map, or no username | no output; the next helper answers |
| identifier listed without a token | warning on stderr, no output |
| map file missing | no output |
| map file unreadable | warning on stderr, no output |
| `store` / `erase` | ignored; nothing is saved or deleted |

Credential helpers only run for HTTPS remotes. SSH remotes authenticate with
SSH keys instead.
