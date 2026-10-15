use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MAP: &str = "work@ TOK_WORK\nside@\nwork@ TOK_DUP\n";
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
    run_args(&[action], input, vars)
}

fn run_args(args: &[&str], input: &str, vars: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_git-credential-ghpat"));
    cmd.args(args);
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
    // The helper may exit without reading stdin, e.g. on a usage error.
    if let Err(e) = child.stdin.take().unwrap().write_all(input.as_bytes()) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
    }
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
fn get_known_profile() {
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
    assert!(stderr(&out).contains("no token for 'side@'"));
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
            &format!("work@ {token}\n"),
        );
        ("APPDATA", base)
    } else {
        let base = dir.join("home");
        dir.write(
            "home/.config/git-credential-ghpat/credentials",
            &format!("work@ {token}\n"),
        );
        ("HOME", base)
    }
}

#[test]
fn path_from_xdg_config_home() {
    let dir = TempDir::new("xdg");
    dir.write("xdg/git-credential-ghpat/credentials", "work@ TOK_XDG\n");
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
    let map = dir.write("map", "work@ TOK_FILE\n");
    dir.write("xdg/git-credential-ghpat/credentials", "work@ TOK_XDG\n");
    let xdg = dir.join("xdg");
    let out = run(
        "get",
        "username=work\n\n",
        &[("GHPAT_FILE", &map), ("XDG_CONFIG_HOME", &xdg)],
    );
    assert_eq!(password(&out), Some("TOK_FILE"));
}

/// Run a management subcommand against the table at map.
fn cmd(args: &[&str], input: &str, map: &Path) -> Output {
    run_args(args, input, &[("GHPAT_FILE", map)])
}

#[test]
fn set_get_del_round_trip() {
    let dir = TempDir::new("round-trip");
    let map = dir.join("new-dir").join("credentials");

    let out = cmd(&["set-token", "work@"], "TOK1\n", &map);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(&map).unwrap(), "work@ TOK1\n");

    let out = cmd(&["get-token", "work@"], "", &map);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "TOK1\n");

    cmd(&["set-token", "side"], "TOK_SIDE", &map);
    cmd(&["set-token", "work@"], "  TOK2  \n", &map);
    assert_eq!(
        fs::read_to_string(&map).unwrap(),
        "work@ TOK2\nside TOK_SIDE\n"
    );
    let out = helper("get", "username=work\n\n", &map);
    assert_eq!(password(&out), Some("TOK2"));

    let out = cmd(&["del-token", "work@"], "", &map);
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(&map).unwrap(), "side TOK_SIDE\n");

    for args in [["get-token", "work@"], ["del-token", "work@"]] {
        let out = cmd(&args, "", &map);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(stdout(&out), "");
        assert!(stderr(&out).contains("no token for 'work@'"));
    }
}

#[test]
fn get_and_del_on_missing_file_fail() {
    let dir = TempDir::new("no-file");
    let map = dir.join("credentials");
    for command in ["get-token", "del-token"] {
        let out = cmd(&[command, "work"], "", &map);
        assert_eq!(out.status.code(), Some(1));
        assert!(stderr(&out).contains("no token for 'work'"));
    }
    assert!(!map.exists());
}

#[test]
fn set_token_keeps_other_lines() {
    let dir = TempDir::new("keep");
    let map = dir.write("map", "\u{feff}# mine\r\nwork OLD\r\n\r\nside B\r\n");
    let out = cmd(&["set-token", "work"], "NEW\r\n", &map);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        fs::read_to_string(&map).unwrap(),
        "\u{feff}# mine\r\nwork NEW\r\n\r\nside B\r\n"
    );
}

#[test]
fn set_token_rejects_bad_tokens() {
    let dir = TempDir::new("bad-token");
    let map = dir.join("credentials");
    for (input, message) in [
        ("", "no token given"),
        ("\n", "no token given"),
        ("a b\n", "cannot contain whitespace"),
    ] {
        let out = cmd(&["set-token", "work"], input, &map);
        assert_eq!(out.status.code(), Some(1), "{input:?}");
        assert!(stderr(&out).contains(message), "{input:?}");
    }
    assert!(!map.exists());
}

#[test]
fn bad_arguments_print_usage() {
    let dir = TempDir::new("usage");
    let map = dir.join("credentials");
    for args in [
        &[][..],
        &["set-token"],
        &["get-token", "a", "b"],
        &["set-token", "#work"],
        &["set-token", "a b"],
        &["del-token", ""],
        &["set-token", "/repo"],
        &["set-token", "owner/"],
        &["get-token", "a/b/c"],
        &["del-token", "a@b@c"],
        &["list-tokens", "work"],
    ] {
        let out = cmd(args, "TOK\n", &map);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(stdout(&out), "");
        assert!(!stderr(&out).is_empty(), "{args:?}");
    }
    assert!(!map.exists());

    let out = cmd(&["--help"], "", &map);
    assert!(out.status.success());
    assert!(stdout(&out).contains("set-token"));
}

#[test]
fn set_token_uses_default_path() {
    let dir = TempDir::new("default-path");
    let xdg = dir.join("xdg");
    let out = run_args(
        &["set-token", "work"],
        "TOK\n",
        &[("XDG_CONFIG_HOME", &xdg)],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let map = xdg.join("git-credential-ghpat").join("credentials");
    assert_eq!(fs::read_to_string(map).unwrap(), "work TOK\n");
}

#[cfg(unix)]
#[test]
fn set_token_creates_private_files() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("private");
    let map = dir.join("new-dir").join("credentials");
    cmd(&["set-token", "work"], "TOK\n", &map);
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&map), 0o600);
    assert_eq!(mode(map.parent().unwrap()), 0o700);

    // Replacing a world-readable table tightens it.
    let open = dir.write("open", "work OLD\n");
    fs::set_permissions(&open, fs::Permissions::from_mode(0o644)).unwrap();
    cmd(&["set-token", "work"], "NEW\n", &open);
    assert_eq!(mode(&open), 0o600);
}

#[cfg(unix)]
#[test]
fn set_token_writes_through_symlink() {
    let dir = TempDir::new("symlink");
    let target = dir.write("real", "work OLD\n");
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let out = cmd(&["set-token", "work"], "NEW\n", &link);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "work NEW\n");
}

#[test]
fn set_token_leaves_no_temp_files() {
    let dir = TempDir::new("no-temp");
    let map = dir.join("credentials");
    cmd(&["set-token", "work"], "TOK\n", &map);
    cmd(&["del-token", "work"], "", &map);
    let names: Vec<_> = fs::read_dir(&*dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["credentials"]);
}

#[test]
fn list_tokens_prints_names_only() {
    let dir = TempDir::new("list");
    let map = dir.write("map", "# c\nwork A\nside B\nwork C\nbare\n");
    let out = cmd(&["list-tokens"], "", &map);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "work\nside\nbare\n");

    let out = cmd(&["list-tokens"], "", &dir.join("missing"));
    assert!(out.status.success());
    assert_eq!(stdout(&out), "");
    assert_eq!(stderr(&out), "");
}

#[test]
fn set_token_replaces_only_the_last_entry() {
    let dir = TempDir::new("last");
    let map = dir.write("map", "work OLD1\nside B\nwork OLD2\n");
    cmd(&["set-token", "work"], "NEW\n", &map);
    assert_eq!(
        fs::read_to_string(&map).unwrap(),
        "work OLD1\nside B\nwork NEW\n"
    );
    let out = cmd(&["get-token", "work"], "", &map);
    assert_eq!(stdout(&out), "NEW\n");
}

#[test]
fn set_token_from_argument() {
    let dir = TempDir::new("arg");
    let map = dir.join("credentials");
    // With --token, stdin is not read.
    let out = cmd(
        &["set-token", "work", "--token", "TOK_ARG"],
        "TOK_STDIN\n",
        &map,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let out = cmd(&["set-token", "--token=TOK_EQ", "side"], "", &map);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        fs::read_to_string(&map).unwrap(),
        "work TOK_ARG\nside TOK_EQ\n"
    );

    for (token, message) in [("", "no token given"), ("a b", "cannot contain whitespace")] {
        let out = cmd(&["set-token", "work", "--token", token], "", &map);
        assert_eq!(out.status.code(), Some(1), "{token:?}");
        assert!(stderr(&out).contains(message), "{token:?}");
    }

    for args in [
        &["set-token", "work", "--token"][..],
        &["get-token", "work", "--token", "T"],
        &["del-token", "work", "--token=T"],
    ] {
        let out = cmd(args, "", &map);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    assert_eq!(
        fs::read_to_string(&map).unwrap(),
        "work TOK_ARG\nside TOK_EQ\n"
    );
}

#[test]
fn get_by_owner_and_repo() {
    let dir = TempDir::new("owner-repo");
    let map = dir.write(
        "map",
        "work@ TOK_WORK\nacme TOK_ACME\nacme/api TOK_API\n@other TOK_OTHER\n",
    );
    for (input, token) in [
        ("username=work\npath=acme/web.git\n\n", Some("TOK_ACME")),
        ("username=work\npath=Acme/API.git\n\n", Some("TOK_API")),
        ("path=acme/api.git/info/lfs\n\n", Some("TOK_API")),
        ("path=other/x\n\n", Some("TOK_OTHER")),
        ("username=work\npath=else/x\n\n", Some("TOK_WORK")),
        // Without a path, owner rules never match.
        ("username=work\n\n", Some("TOK_WORK")),
        ("path=else/x\n\n", None),
        ("host=github.com\n\n", None),
    ] {
        let out = helper("get", input, &map);
        assert!(out.status.success());
        assert_eq!(password(&out), token, "{input:?}");
        assert_eq!(stderr(&out), "", "{input:?}");
    }
}

#[test]
fn get_matches_any_request_with_at() {
    let dir = TempDir::new("catch-all");
    let map = dir.write("map", "work@ TOK_WORK\n@ TOK_ANY\n");
    for input in ["\n", "username=work\npath=acme/api\n\n"] {
        let out = helper("get", input, &map);
        assert_eq!(password(&out), Some("TOK_ANY"), "{input:?}");
    }
}

#[test]
fn invalid_patterns_warn_and_are_skipped() {
    let dir = TempDir::new("invalid");
    let map = dir.write("map", "work@ TOK_WORK\n/api TOK_BAD\nacme/ TOK_BAD\n");
    let out = helper("get", "username=work\npath=acme/api\n\n", &map);
    assert!(out.status.success());
    assert_eq!(password(&out), Some("TOK_WORK"));
    let err = stderr(&out);
    assert!(err.contains("invalid pattern '/api'"), "{err}");
    assert!(err.contains("invalid pattern 'acme/'"), "{err}");

    let out = cmd(&["list-tokens"], "", &map);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "work@\n/api\nacme/\n");
    assert!(stderr(&out).contains("invalid pattern '/api'"));
}
