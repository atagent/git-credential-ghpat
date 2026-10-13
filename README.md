# git-credential-ghpat

A git credential helper for using several GitHub PATs with one account.

Git's username for a request is used as an **identifier**. The helper looks
it up in a credentials file and answers with that PAT, sending `token` as the
username, so the identifier never reaches GitHub.

```
git → username=work → ghpat → username=token, password=github_pat_…
```

Works on Linux and Windows.

## Install

Download the program from
[Releases](https://github.com/atagent/git-credential-ghpat/releases) and put
it in the `.local\bin` folder in your home folder (`.local/bin` on Linux),
where tools like uv and ruff also go.

### Windows

1. On the [latest release](https://github.com/atagent/git-credential-ghpat/releases/latest),
   download the file ending in `x86_64-pc-windows-msvc.zip`.
2. Extract `git-credential-ghpat.exe` from the zip file and place it in
   `C:\Users\USERNAME\.local\bin`, creating the folder if needed.

### Linux

1. On the [latest release](https://github.com/atagent/git-credential-ghpat/releases/latest),
   download the file ending in `x86_64-unknown-linux-gnu.tar.gz`.
2. Extract `git-credential-ghpat` from the archive and place it in
   `~/.local/bin`, creating the folder if needed.

### From a terminal

<details>
<summary>The same steps as commands, with checksum verification</summary>

Set `v` to the release you want.

Linux:

```sh
v=v0.1.0
base=https://github.com/atagent/git-credential-ghpat/releases/download/$v
curl -fLO "$base/git-credential-ghpat-$v-x86_64-unknown-linux-gnu.tar.gz"
curl -fLO "$base/SHA256SUMS"
sha256sum --ignore-missing -c SHA256SUMS
tar -xzf "git-credential-ghpat-$v-x86_64-unknown-linux-gnu.tar.gz" git-credential-ghpat
install -D -m 755 git-credential-ghpat ~/.local/bin/git-credential-ghpat
```

Windows (PowerShell):

```powershell
$v = "v0.1.0"
$base = "https://github.com/atagent/git-credential-ghpat/releases/download/$v"
$zip = "git-credential-ghpat-$v-x86_64-pc-windows-msvc.zip"
Invoke-WebRequest "$base/$zip" -OutFile $zip
Invoke-WebRequest "$base/SHA256SUMS" -OutFile SHA256SUMS
Select-String (Get-FileHash $zip).Hash SHA256SUMS   # no output: bad download
Expand-Archive $zip -DestinationPath git-credential-ghpat -Force
New-Item -ItemType Directory -Force "$env:USERPROFILE\.local\bin" | Out-Null
Copy-Item git-credential-ghpat\git-credential-ghpat.exe "$env:USERPROFILE\.local\bin"
```

</details>

### From source

This needs a recent stable [Rust toolchain](https://rustup.rs/):

```sh
cargo install --locked --git https://github.com/atagent/git-credential-ghpat --tag v0.1.0
```

This installs into `~/.cargo/bin` (`C:\Users\USERNAME\.cargo\bin` on Windows),
which rustup puts on `PATH`.

### Check

Open a new terminal (on Windows, PowerShell or Git Bash) and run:

```sh
git-credential-ghpat --help
```

If it prints the usage, git can find the helper too. If the program is not
found, add the folder you put it in to your `PATH`.

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

### Choose a PAT

Set the identifier as the username. It's the name you store the token under
with `set-token` (see [Manage tokens](#manage-tokens)). Any of these works:

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

## Troubleshooting

To see what git gets for an identifier, run this in a terminal (on Windows,
in Git Bash). It prints the username and PAT that git would use.

```sh
printf 'protocol=https\nhost=github.com\nusername=work\n\n' | git credential fill
```

| What you see | Why |
|---|---|
| git asks for a password, or the fallback helper answers | git sent no identifier (see [Choose a PAT](#choose-a-pat)), the identifier isn't in the credentials file, or the file doesn't exist |
| `git: 'credential-ghpat' is not a git command` | the program isn't on your `PATH` (see [Install](#install)) |
| `git-credential-ghpat: no token for 'NAME' in FILE` | the name is listed without a token; run `set-token NAME` |
| `git-credential-ghpat: cannot read FILE: …` | the file exists but can't be read, e.g. because of its permissions |
| the helper is never used | the remote uses SSH (`git@github.com:…`); credential helpers only run for HTTPS remotes |

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md).
