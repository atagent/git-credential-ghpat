use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAP: &str = "work TOK_WORK\nside\nwork TOK_DUP\n";
const PATH_VARS: [&str; 4] = ["GHPAT_FILE", "XDG_CONFIG_HOME", "HOME", "APPDATA"];

/// A per-test temp directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ghpat-{}-{test}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    /// Write a file at a path relative to this directory and return its path.
    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Run the helper with only the given path variables set.
fn run(action: &str, input: &str, vars: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_git-credential-ghpat"));
    cmd.arg(action);
    for var in PATH_VARS {
        cmd.env_remove(var);
    }
    for (var, value) in vars {
        cmd.env(var, value);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn helper(action: &str, input: &str, map: &Path) -> Output {
    run(action, input, &[("GHPAT_FILE", map)])
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap()
}

fn password(output: &Output) -> Option<&str> {
    stdout(output)
        .lines()
        .find_map(|line| line.strip_prefix("password="))
}

#[test]
fn get_known_identifier() {
    let dir = TempDir::new("known");
    let map = dir.write("map", MAP);
    let out = helper(
        "get",
        "protocol=https\nhost=github.com\nusername=work\n\n",
        &map,
    );
    assert!(out.status.success());
    assert_eq!(stdout(&out), "username=token\npassword=TOK_DUP\n");
}

#[test]
fn get_unknown_or_missing_username_is_silent() {
    let dir = TempDir::new("unknown");
    let map = dir.write("map", MAP);
    for input in [
        "username=nobody\n\n",
        "host=github.com\n\n",
        "username=\n\n",
    ] {
        let out = helper("get", input, &map);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "");
        assert_eq!(stderr(&out), "");
    }
}

#[test]
fn get_identifier_without_token_warns() {
    let dir = TempDir::new("bare");
    let map = dir.write("map", MAP);
    let out = helper("get", "username=side\n\n", &map);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("no token for 'side'"));
}

#[test]
fn missing_map_is_silent() {
    let dir = TempDir::new("missing");
    let out = helper("get", "username=work\n\n", &dir.join("map"));
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert_eq!(stderr(&out), "");
}

#[test]
fn unreadable_map_warns() {
    // A directory cannot be read as a file on any platform.
    let dir = TempDir::new("unreadable");
    let out = helper("get", "username=work\n\n", &dir);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("cannot read"));
}

#[test]
fn store_and_erase_do_nothing() {
    let dir = TempDir::new("store");
    let map = dir.write("map", MAP);
    for action in ["store", "erase", ""] {
        let out = helper(action, "username=work\npassword=p\n\n", &map);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "");
        assert_eq!(fs::read_to_string(&map).unwrap(), MAP);
    }
}

/// The platform's fallback when XDG_CONFIG_HOME is unset: %APPDATA% on
/// Windows, $HOME/.config elsewhere.
fn fallback(dir: &TempDir, token: &str) -> (&'static str, PathBuf) {
    if cfg!(windows) {
        let base = dir.join("appdata");
        dir.write(
            "appdata/git-credential-ghpat/credentials",
            &format!("work {token}\n"),
        );
        ("APPDATA", base)
    } else {
        let base = dir.join("home");
        dir.write(
            "home/.config/git-credential-ghpat/credentials",
            &format!("work {token}\n"),
        );
        ("HOME", base)
    }
}

#[test]
fn path_from_xdg_config_home() {
    let dir = TempDir::new("xdg");
    dir.write("xdg/git-credential-ghpat/credentials", "work TOK_XDG\n");
    let (var, base) = fallback(&dir, "TOK_FALLBACK");
    let xdg = dir.join("xdg");
    let out = run(
        "get",
        "username=work\n\n",
        &[("XDG_CONFIG_HOME", &xdg), (var, &base)],
    );
    assert_eq!(password(&out), Some("TOK_XDG"));
}

#[test]
fn path_falls_back_without_xdg_config_home() {
    let dir = TempDir::new("fallback");
    let (var, base) = fallback(&dir, "TOK_FALLBACK");
    let out = run("get", "username=work\n\n", &[(var, &base)]);
    assert_eq!(password(&out), Some("TOK_FALLBACK"));
}

#[test]
fn path_ignores_relative_xdg_config_home() {
    let dir = TempDir::new("relative");
    let (var, base) = fallback(&dir, "TOK_FALLBACK");
    let out = run(
        "get",
        "username=work\n\n",
        &[("XDG_CONFIG_HOME", Path::new("relative")), (var, &base)],
    );
    assert_eq!(password(&out), Some("TOK_FALLBACK"));
}

#[test]
fn path_from_ghpat_file_wins() {
    let dir = TempDir::new("override");
    let map = dir.write("map", "work TOK_FILE\n");
    dir.write("xdg/git-credential-ghpat/credentials", "work TOK_XDG\n");
    let xdg = dir.join("xdg");
    let out = run(
        "get",
        "username=work\n\n",
        &[("GHPAT_FILE", &map), ("XDG_CONFIG_HOME", &xdg)],
    );
    assert_eq!(password(&out), Some("TOK_FILE"));
}
