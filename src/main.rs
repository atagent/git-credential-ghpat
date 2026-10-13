//! git-credential-ghpat: git credential helper that picks a GitHub PAT by
//! identifier. Git sends the identifier as the username; the helper looks it
//! up in a two-column map file and answers with the PAT, rewriting the
//! username to "token" so the identifier never reaches GitHub.
//!
//! Map file ($GHPAT_FILE, default ~/.config/git/ghpat):
//!
//!   # identifier  token
//!   work          github_pat_AAAA...
//!   side          github_pat_BBBB...
//!
//! If an identifier is listed more than once, the last entry wins.
//! Unknown identifiers produce no output, so the next helper can answer.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const PROG: &str = "git-credential-ghpat";
const USERNAME: &str = "token";

fn env_nonempty(key: &str) -> Option<OsString> {
    env::var_os(key).filter(|value| !value.is_empty())
}

/// Git for Windows sets HOME for the processes it starts; USERPROFILE covers
/// running the helper by hand.
fn home_dir() -> Option<PathBuf> {
    env_nonempty("HOME")
        .or_else(|| cfg!(windows).then(|| env_nonempty("USERPROFILE")).flatten())
        .map(PathBuf::from)
}

fn get_ghpat_path() -> Option<PathBuf> {
    if let Some(path) = env_nonempty("GHPAT_FILE") {
        return Some(path.into());
    }
    let config_home = env_nonempty("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))?;
    Some(config_home.join("git").join("ghpat"))
}

/// Return the username from git's key=value lines, up to the blank line that
/// ends them, or None if there is none. The last one wins.
fn read_username(input: impl BufRead) -> io::Result<Option<String>> {
    let mut username = None;
    for line in input.split(b'\n') {
        let line = line?;
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix(b"username=") {
            username = Some(String::from_utf8_lossy(value).into_owned());
        }
    }
    Ok(username)
}

/// Return the token listed for name, or None if name is not listed.
/// The last entry wins; an entry without a token gives "".
fn find_token<'a>(contents: &'a str, name: &str) -> Option<&'a str> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut token = None;
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some(name) {
            token = Some(parts.next().unwrap_or(""));
        }
    }
    token
}

fn run() -> io::Result<()> {
    let action = env::args_os().nth(1);
    let user = read_username(io::stdin().lock())?;
    // Nothing is saved, so only "get" has work to do.
    if action.as_deref() != Some(OsStr::new("get")) {
        return Ok(());
    }
    let Some(user) = user.filter(|user| !user.is_empty()) else {
        return Ok(());
    };
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

    match find_token(&contents, &user) {
        None => {}
        Some("") => eprintln!("{PROG}: no token for '{user}' in {}", path.display()),
        Some(token) => write!(
            io::stdout().lock(),
            "username={USERNAME}\npassword={token}\n"
        )?,
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{PROG}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_last_one_wins() {
        let input = b"protocol=https\nusername=a\nusername=b\n\nusername=c\n";
        assert_eq!(read_username(&input[..]).unwrap().as_deref(), Some("b"));
    }

    #[test]
    fn username_accepts_crlf_and_missing_blank_line() {
        let input = b"host=github.com\r\nusername=work\r\n";
        assert_eq!(read_username(&input[..]).unwrap().as_deref(), Some("work"));
    }

    #[test]
    fn username_ignores_other_keys() {
        let input = b"path=a=b\npassword=username=x\n\n";
        assert_eq!(read_username(&input[..]).unwrap(), None);
    }

    #[test]
    fn token_lookup() {
        let map =
            "\u{feff}# comment\r\n\r\nwork  A\r\nside\tB extra\r\nwork C\r\nbare\r\n  last D  ";
        assert_eq!(find_token(map, "work"), Some("C"));
        assert_eq!(find_token(map, "side"), Some("B"));
        assert_eq!(find_token(map, "last"), Some("D"));
        assert_eq!(find_token(map, "bare"), Some(""));
        assert_eq!(find_token(map, "nobody"), None);
    }
}
