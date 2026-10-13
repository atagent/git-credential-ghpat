use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAP: &str = "work TOK_WORK\nside\nwork TOK_DUP\n";

/// A map file in its own temp directory, removed on drop.
struct TempMap(PathBuf);

impl Deref for TempMap {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempMap {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.parent().unwrap());
    }
}

fn temp_map(test: &str, contents: &str) -> TempMap {
    let dir = std::env::temp_dir().join(format!("ghpat-{}-{test}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ghpat");
    fs::write(&path, contents).unwrap();
    TempMap(path)
}

fn helper(action: &str, input: &str, map: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_git-credential-ghpat"))
        .arg(action)
        .env("GHPAT_FILE", map)
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

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap()
}

#[test]
fn get_known_identifier() {
    let map = temp_map("known", MAP);
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
    let map = temp_map("unknown", MAP);
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
    let map = temp_map("bare", MAP);
    let out = helper("get", "username=side\n\n", &map);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("no token for 'side'"));
}

#[test]
fn missing_map_is_silent() {
    let map = std::env::temp_dir().join("ghpat-does-not-exist");
    let out = helper("get", "username=work\n\n", &map);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert_eq!(stderr(&out), "");
}

#[test]
fn unreadable_map_warns() {
    // A directory cannot be read as a file on any platform.
    let map = temp_map("unreadable", MAP);
    let out = helper("get", "username=work\n\n", map.parent().unwrap());
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("cannot read"));
}

#[test]
fn store_and_erase_do_nothing() {
    let map = temp_map("store", MAP);
    for action in ["store", "erase", ""] {
        let out = helper(action, "username=work\npassword=p\n\n", &map);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "");
        assert_eq!(fs::read_to_string(&*map).unwrap(), MAP);
    }
}
