# git-credential-ghpat

A git credential helper for using several GitHub PATs with one account.

The helper picks a PAT by **pattern**, `[profile@][owner[/repo]]`. The profile
is git's username for the request, and the owner and repo come from the repo's
URL. The helper answers with the PAT and sends `token` as the username, so the
profile never reaches GitHub.

```
git → username=work, path=acme/api.git → ghpat → username=token, password=github_pat_…
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
v=v0.2.0
base=https://github.com/atagent/git-credential-ghpat/releases/download/$v
curl -fLO "$base/git-credential-ghpat-$v-x86_64-unknown-linux-gnu.tar.gz"
curl -fLO "$base/SHA256SUMS"
sha256sum --ignore-missing -c SHA256SUMS
tar -xzf "git-credential-ghpat-$v-x86_64-unknown-linux-gnu.tar.gz" git-credential-ghpat
install -D -m 755 git-credential-ghpat ~/.local/bin/git-credential-ghpat
```

Windows (PowerShell):

```powershell
$v = "v0.2.0"
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
cargo install --locked --git https://github.com/atagent/git-credential-ghpat --tag v0.2.0
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
    useHttpPath = true                 # needed for owner and repo patterns
```

- `helper =` clears any helpers configured earlier, including the `manager`
  helper that the Git for Windows installer sets up.
- `ghpat` must come first. The fallback only answers requests that no
  pattern matches.
- `useHttpPath = true` makes git send the repo path (`owner/repo.git`).
  Without it, patterns with an owner never match.

### Choose a PAT

Store each token under a pattern with `set-token` (see
[Manage tokens](#manage-tokens)). Each part of a pattern you leave out
matches anything:

| Pattern | Matches |
|---|---|
| `acme` | every repo of the owner `acme` |
| `acme/api` | the repo `acme/api` only |
| `work@` | requests with the username `work`, any repo |
| `work@acme` | username `work` and owner `acme` |
| `@` | every request |

`@acme` and `@acme/api` are the same as `acme` and `acme/api`.

Owner and repo patterns need only `useHttpPath = true`. A profile pattern
needs git to send that username. Any of these works:

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
git-credential-ghpat list-tokens        # print the patterns, never the tokens
git-credential-ghpat set-token acme     # add, or replace the entry in use
git-credential-ghpat get-token acme     # print the token
git-credential-ghpat del-token acme     # remove every entry for the pattern
```

These subcommands treat a pattern as a name: `get-token acme` prints the
token stored under `acme`, not the one git would get for an `acme` repo.

- `set-token` prompts for the token in a terminal, without echoing it.
  Otherwise it reads the first line of stdin:
  `git-credential-ghpat set-token acme < token.txt`.
- `set-token acme --token TOKEN` (or `--token=TOKEN`) takes the token from
  the argument instead. This is convenient, but the token ends up in your
  shell history and is visible in the process list while the command runs.
- `set-token` replaces only the last entry for a pattern. Earlier duplicates
  are left alone; a new pattern is appended.
- `get-token` is for scripts, e.g.
  `GH_TOKEN=$(git-credential-ghpat get-token acme) gh pr list`
- Edits keep all other lines as they are. The file is replaced atomically,
  created readable only by you (`0600` in a `0700` directory on Linux), and a
  symlink is written through rather than replaced.
- `get-token` and `del-token` exit with status 1 if the pattern isn't listed;
  bad arguments, including an invalid pattern, exit with status 2.

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
# pattern   token
@           github_pat_AAAA...
work@       github_pat_BBBB...
acme        github_pat_CCCC...
acme/api    github_pat_DDDD...
```

- Each line has two columns separated by whitespace. Blank lines and lines
  starting with `#` are ignored.
- The **last** entry that matches wins, wherever it is in the file. A more
  specific pattern doesn't take precedence, so put broad patterns like `@`
  first. In the example, `acme/api` gets `DDDD` even with the username
  `work`, and username `work` on a repo outside `acme` gets `BBBB`.
- The profile is case-sensitive. Owner and repo are not, as on GitHub.
- A trailing `.git` is ignored, and only the first two parts of the path
  count, so Git LFS requests (`acme/api.git/info/lfs`) match `acme/api`.
- A line with an invalid pattern (`/repo`, `owner/`, `a/b/c`, `a@b@c`) is
  ignored, and the helper and `list-tokens` print a warning about it.
- Host and protocol are not checked. Configure git to use the helper only
  for the sites you want it for.
- Windows line endings (CRLF) and a UTF-8 BOM are accepted.

### Upgrading from 0.1

Version 0.1 matched only the username, written without `@`. A line
`work TOKEN` now means the owner `work`. Change such lines to `work@ TOKEN`
to keep matching the username.

## Troubleshooting

To see what git gets for a request, run this in a terminal (on Windows, in
Git Bash). It prints the username and PAT that git would use. Leave out the
`username=` or `path=` line to test without it.

```sh
printf 'protocol=https\nhost=github.com\nusername=work\npath=acme/api.git\n\n' | git credential fill
```

| What you see | Why |
|---|---|
| git asks for a password, or the fallback helper answers | no pattern matches the request (see [Choose a PAT](#choose-a-pat)), or the file doesn't exist |
| owner or repo patterns never match | `useHttpPath = true` isn't set (see [Configure git](#configure-git)) |
| a profile pattern never matches | git sends no username, or the line still has the 0.1 form (see [Upgrading from 0.1](#upgrading-from-01)) |
| `git: 'credential-ghpat' is not a git command` | the program isn't on your `PATH` (see [Install](#install)) |
| `git-credential-ghpat: no token for 'PATTERN' in FILE` | the matching entry has no token; run `set-token PATTERN` |
| `git-credential-ghpat: invalid pattern 'X' in FILE, ignored` | fix or remove that line |
| `git-credential-ghpat: cannot read FILE: …` | the file exists but can't be read, e.g. because of its permissions |
| the helper is never used | the remote uses SSH (`git@github.com:…`); credential helpers only run for HTTPS remotes |

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md).
