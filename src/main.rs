//! git-credential-ghpat: git credential helper that picks a GitHub PAT by
//! pattern. Git sends a username (the profile) and, with
//! credential.useHttpPath, the repo path; the helper finds the last matching
//! `[profile@][owner[/repo]]` entry in a credentials table and answers with
//! its PAT, rewriting the username to "token" so the profile never reaches
//! GitHub.
//!
//! The table is $GHPAT_FILE, or credentials in the config directory (see
//! get_ghpat_path). It is managed with the list-tokens, get-token,
//! set-token, and del-token subcommands; see table.rs for the format.
//!
//! Requests that match no entry produce no output, so the next helper can
//! answer.

mod table;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

const PROG: &str = "git-credential-ghpat";
const USERNAME: &str = "token";

const USAGE: &str = "\
usage: git-credential-ghpat list-tokens
       git-credential-ghpat get-token <pattern>
       git-credential-ghpat set-token <pattern> [--token <token>]
       git-credential-ghpat del-token <pattern>

A pattern is [profile@][owner[/repo]], e.g. work@, acme, or work@acme/api.

Without --token, set-token prompts for the token in a terminal, without
echoing it, and otherwise reads it from stdin. --token puts the token in
your shell history and the process list.

Git runs it as a credential helper with get, store, or erase.
";

fn env_nonempty(key: &str) -> Option<OsString> {
    env::var_os(key).filter(|value| !value.is_empty())
}

/// An absolute directory from the environment. Relative values are ignored,
/// as the XDG base directory spec requires.
fn env_dir(key: &str) -> Option<PathBuf> {
    env_nonempty(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// $XDG_CONFIG_HOME, falling back to %APPDATA% on Windows and to
/// $HOME/.config elsewhere.
fn config_dir() -> Option<PathBuf> {
    env_dir("XDG_CONFIG_HOME").or_else(|| {
        if cfg!(windows) {
            env_dir("APPDATA")
        } else {
            env_dir("HOME").map(|home| home.join(".config"))
        }
    })
}

fn get_ghpat_path() -> Option<PathBuf> {
    if let Some(path) = env_nonempty("GHPAT_FILE") {
        return Some(path.into());
    }
    Some(config_dir()?.join(PROG).join("credentials"))
}

/// The parts of git's request that patterns match on.
#[derive(Debug, Default, PartialEq)]
struct Request {
    username: Option<String>,
    path: Option<String>,
}

/// Return the username and path from git's key=value lines, up to the blank
/// line that ends them. The last of each wins.
fn read_request(input: impl BufRead) -> io::Result<Request> {
    let mut request = Request::default();
    let value = |value: &[u8]| Some(String::from_utf8_lossy(value).into_owned());
    for line in input.split(b'\n') {
        let line = line?;
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        if line.is_empty() {
            break;
        }
        if let Some(username) = line.strip_prefix(b"username=") {
            request.username = value(username);
        } else if let Some(path) = line.strip_prefix(b"path=") {
            request.path = value(path);
        }
    }
    Ok(request)
}

/// Warn about entries whose pattern doesn't parse, since they never match.
fn warn_invalid(contents: &str, path: &Path) {
    for pattern in table::invalid(contents) {
        eprintln!(
            "{PROG}: invalid pattern '{pattern}' in {}, ignored",
            path.display()
        );
    }
}

/// The credential helper side: answer git's "get", ignore everything else.
/// Problems are reported on stderr but never fail, so the next helper can
/// still answer.
fn helper(action: &str) -> io::Result<()> {
    let request = read_request(io::stdin().lock())?;
    // Nothing is saved, so only "get" has work to do.
    if action != "get" {
        return Ok(());
    }
    let Some(path) = get_ghpat_path() else {
        return Ok(());
    };

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            eprintln!("{PROG}: cannot read {}: {e}", path.display());
            return Ok(());
        }
    };

    warn_invalid(&contents, &path);
    let found = table::lookup(
        &contents,
        request.username.as_deref(),
        request.path.as_deref(),
    );
    match found {
        None => {}
        Some((pattern, "")) => eprintln!("{PROG}: {}", no_token(pattern, &path)),
        Some((_, token)) => write!(
            io::stdout().lock(),
            "username={USERNAME}\npassword={token}\n"
        )?,
    }
    Ok(())
}

fn table_path() -> Result<PathBuf, String> {
    get_ghpat_path().ok_or_else(|| {
        "cannot locate the credentials file: no config directory; set GHPAT_FILE".into()
    })
}

/// The table's contents, or "" if the file doesn't exist yet.
fn read_table(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Replace the file at path with contents, readable only by the user.
/// The new contents go to a temporary file that is renamed into place, so
/// the table is never left half-written. A symlink is written through, not
/// replaced.
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut dir_builder = fs::DirBuilder::new();
    dir_builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        dir_builder.mode(0o700);
    }
    dir_builder.create(dir)?;

    let Some(name) = path.file_name() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a file"));
    };
    let mut tmp_name = OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", process::id()));
    let tmp = dir.join(tmp_name);

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options
        .open(&tmp)
        .and_then(|mut file| {
            file.write_all(contents.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, &path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn no_token(name: &str, path: &Path) -> String {
    format!("no token for '{name}' in {}", path.display())
}

/// Prompt for the token without echoing it if stdin is a terminal; otherwise
/// read the first line of stdin.
fn read_token(name: &str) -> io::Result<String> {
    let stdin = io::stdin();
    let line = if stdin.is_terminal() {
        rpassword::prompt_password(format!("Token for '{name}': "))?
    } else {
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        line
    };
    Ok(line.trim().to_owned())
}

fn list_tokens() -> Result<(), String> {
    let path = table_path()?;
    let contents = read_table(&path)?;
    warn_invalid(&contents, &path);
    let mut stdout = io::stdout().lock();
    for name in table::names(&contents) {
        writeln!(stdout, "{name}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn get_token(name: &str) -> Result<(), String> {
    let path = table_path()?;
    match table::find(&read_table(&path)?, name) {
        Some(token) if !token.is_empty() => {
            writeln!(io::stdout().lock(), "{token}").map_err(|e| e.to_string())
        }
        _ => Err(no_token(name, &path)),
    }
}

/// Set name to the given token, or to one read by read_token if none is given.
fn set_token(name: &str, token: Option<&str>) -> Result<(), String> {
    let token = match token {
        Some(token) => token.to_owned(),
        None => read_token(name).map_err(|e| format!("cannot read the token: {e}"))?,
    };
    if token.is_empty() {
        return Err("no token given".into());
    }
    if !table::valid_token(&token) {
        return Err("a token cannot contain whitespace".into());
    }
    let path = table_path()?;
    let contents = table::set(&read_table(&path)?, name, &token);
    write_private(&path, &contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn del_token(name: &str) -> Result<(), String> {
    let path = table_path()?;
    let Some(contents) = table::remove(&read_table(&path)?, name) else {
        return Err(no_token(name, &path));
    };
    write_private(&path, &contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn usage() -> ExitCode {
    eprint!("{USAGE}");
    ExitCode::from(2)
}

/// The <pattern> and, if allowed, the --token value from a subcommand's
/// arguments, or None if they don't parse.
fn parse_name_args(args: &[OsString], allow_token: bool) -> Option<(&str, Option<&str>)> {
    let mut name = None;
    let mut token = None;
    let mut args = args.iter().map(|arg| arg.to_str());
    while let Some(arg) = args.next() {
        let arg = arg?;
        if allow_token && token.is_none() && arg == "--token" {
            token = Some(args.next()??);
        } else if let Some(value) = arg
            .strip_prefix("--token=")
            .filter(|_| allow_token && token.is_none())
        {
            token = Some(value);
        } else if name.is_none() && !arg.starts_with('-') {
            name = Some(arg);
        } else {
            return None;
        }
    }
    Some((name?, token))
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let command = args.first().map(|arg| arg.to_string_lossy());

    let result = match command.as_deref() {
        None => return usage(),
        Some("-h" | "--help") => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("list-tokens") => {
            if args.len() != 1 {
                return usage();
            }
            list_tokens()
        }
        Some(command @ ("get-token" | "set-token" | "del-token")) => {
            let Some((name, token)) = parse_name_args(&args[1..], command == "set-token") else {
                return usage();
            };
            if !table::valid_pattern(name) {
                eprintln!("{PROG}: invalid pattern '{name}'; expected [profile@][owner[/repo]]");
                return ExitCode::from(2);
            }
            match command {
                "get-token" => get_token(name),
                "set-token" => set_token(name, token),
                _ => del_token(name),
            }
        }
        Some(action) => helper(action).map_err(|e| e.to_string()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{PROG}: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn parse_name_and_token() {
        let parse = |list: &[&str], allow| {
            parse_name_args(&args(list), allow).map(|(n, t)| (n.to_owned(), t.map(str::to_owned)))
        };
        let ok = |n: &str, t: Option<&str>| Some((n.to_owned(), t.map(str::to_owned)));
        assert_eq!(parse(&["work"], false), ok("work", None));
        assert_eq!(
            parse(&["work", "--token", "T"], true),
            ok("work", Some("T"))
        );
        assert_eq!(
            parse(&["--token", "T", "work"], true),
            ok("work", Some("T"))
        );
        assert_eq!(parse(&["work", "--token=T"], true), ok("work", Some("T")));
        assert_eq!(
            parse(&["work", "--token", "-T"], true),
            ok("work", Some("-T"))
        );
        assert_eq!(parse(&["work", "--token="], true), ok("work", Some("")));
        for list in [
            &[][..],
            &["--token", "T"],
            &["work", "--token"],
            &["work", "--token", "A", "--token", "B"],
            &["work", "--token=A", "--token=B"],
            &["work", "other"],
            &["work", "--other"],
            &["-w"],
        ] {
            assert_eq!(parse(list, true), None, "{list:?}");
        }
        assert_eq!(parse(&["work", "--token", "T"], false), None);
        assert_eq!(parse(&["work", "--token=T"], false), None);
    }

    fn request(username: Option<&str>, path: Option<&str>) -> Request {
        Request {
            username: username.map(str::to_owned),
            path: path.map(str::to_owned),
        }
    }

    #[test]
    fn request_last_one_wins() {
        let input =
            b"protocol=https\nusername=a\npath=x/y\nusername=b\npath=o/r.git\n\nusername=c\n";
        assert_eq!(
            read_request(&input[..]).unwrap(),
            request(Some("b"), Some("o/r.git"))
        );
    }

    #[test]
    fn request_accepts_crlf_and_missing_blank_line() {
        let input = b"host=github.com\r\nusername=work\r\npath=o/r\r\n";
        assert_eq!(
            read_request(&input[..]).unwrap(),
            request(Some("work"), Some("o/r"))
        );
    }

    #[test]
    fn request_ignores_other_keys() {
        let input = b"wwwauth[]=path=a\npassword=username=x\n\n";
        assert_eq!(read_request(&input[..]).unwrap(), request(None, None));
    }
}
