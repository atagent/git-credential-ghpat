# git-credential-ghpat

A git credential helper for using several GitHub PATs with one account.

Git's username for a request is used as an **identifier**. The helper looks
it up in a credentials file and answers with that PAT, sending `token` as the
username, so the identifier never reaches GitHub.

```
git → username=work → ghpat → username=token, password=github_pat_…
```

Works on Linux and Windows. Its only dependency is
[`rpassword`](https://crates.io/crates/rpassword), for reading a token
without echoing it.

## Install

```sh
cargo install --path .
```

This installs `git-credential-ghpat` (`git-credential-ghpat.exe` on Windows)
into `~/.cargo/bin`. That directory must be on your `PATH` so git can find the
helper.

## Credentials file

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

- Each line has two columns separated by whitespace. Blank lines and lines
  starting with `#` are ignored.
- If an identifier is listed more than once, the **last** entry wins.
- Windows line endings (CRLF) and a UTF-8 BOM are accepted.

## Manage tokens

```sh
git-credential-ghpat list-tokens       # print the names, never the tokens
git-credential-ghpat set-token work    # add, or replace the entry in use
git-credential-ghpat get-token work    # print the token
git-credential-ghpat del-token work    # remove every entry for the name
```

- `set-token` prompts for the token in a terminal, without echoing it.
  Otherwise it reads the first line of stdin:
  `git-credential-ghpat set-token work < token.txt`.
- `set-token work --token TOKEN` (or `--token=TOKEN`) takes the token from
  the argument instead. This is convenient, but the token ends up in your
  shell history and is visible in the process list while the command runs.
- `set-token` replaces only the last entry for a name, the one that wins.
  Earlier duplicates are left alone; a new name is appended.
- `get-token` is for scripts, e.g.
  `GH_TOKEN=$(git-credential-ghpat get-token work) gh pr list`
- Edits keep all other lines as they are. The file is replaced atomically,
  created readable only by you (`0600` in a `0700` directory on Linux), and a
  symlink is written through rather than replaced.
- `get-token` and `del-token` exit with status 1 if the name isn't listed;
  bad arguments exit with status 2.

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

CI runs clippy and the tests on Linux and Windows, and uploads release
binaries for both as workflow artifacts.

## Behavior

| Request | Result |
|---|---|
| identifier is in the file | `username=token`, `password=<PAT>` |
| identifier not in the file, or no username | no output; the next helper answers |
| identifier listed without a token | warning on stderr, no output |
| credentials file missing | no output |
| credentials file unreadable | warning on stderr, no output |
| `store` / `erase` | ignored; nothing is saved or deleted |

Credential helpers only run for HTTPS remotes. SSH remotes authenticate with
SSH keys instead.
