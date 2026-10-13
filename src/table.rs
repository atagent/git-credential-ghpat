//! The credentials table: one `name token` entry per line, separated by
//! whitespace. Blank lines and lines starting with `#` are ignored, and the
//! last entry for a name wins.
//!
//! Edits keep every other line as it is, including comments, order, CRLF
//! line endings, and a leading UTF-8 BOM.

const BOM: char = '\u{feff}';

/// The name an entry line defines, or None for blank and comment lines.
fn key(line: &str) -> Option<&str> {
    line.split_whitespace()
        .next()
        .filter(|k| !k.starts_with('#'))
}

fn split_bom(contents: &str) -> (&str, &str) {
    match contents.strip_prefix(BOM) {
        Some(body) => (&contents[..BOM.len_utf8()], body),
        None => ("", contents),
    }
}

/// A name must be one non-empty word that can't be read as a comment.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('#') && !name.contains(char::is_whitespace)
}

/// A token must be one non-empty word.
pub fn valid_token(token: &str) -> bool {
    !token.is_empty() && !token.contains(char::is_whitespace)
}

/// Return the token listed for name, or None if name is not listed.
/// The last entry wins; an entry without a token gives "".
pub fn find<'a>(contents: &'a str, name: &str) -> Option<&'a str> {
    let (_, body) = split_bom(contents);
    let mut token = None;
    for line in body.lines() {
        if key(line) == Some(name) {
            token = Some(line.split_whitespace().nth(1).unwrap_or(""));
        }
    }
    token
}

/// The listed names, each once, in order of first appearance.
pub fn names(contents: &str) -> Vec<&str> {
    let (_, body) = split_bom(contents);
    let mut names = Vec::new();
    for name in body.lines().filter_map(key) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Return contents with name set to token. The entry replaces the last
/// existing one for name, the one that wins; a new name is appended.
pub fn set(contents: &str, name: &str, token: &str) -> String {
    let (bom, body) = split_bom(contents);
    let eol = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let entry = format!("{name} {token}{eol}");
    let lines: Vec<&str> = body.split_inclusive('\n').collect();

    let mut out = String::from(bom);
    match lines.iter().rposition(|line| key(line) == Some(name)) {
        Some(last) => {
            out.extend(lines[..last].iter().copied());
            out.push_str(&entry);
            out.extend(lines[last + 1..].iter().copied());
        }
        None => {
            out.push_str(body);
            if !body.is_empty() && !body.ends_with('\n') {
                out.push_str(eol);
            }
            out.push_str(&entry);
        }
    }
    out
}

/// Return contents without any entry for name, or None if name is not listed.
pub fn remove(contents: &str, name: &str) -> Option<String> {
    let (bom, body) = split_bom(contents);
    let mut found = false;
    let mut out = String::from(bom);
    for line in body.split_inclusive('\n') {
        if key(line) == Some(name) {
            found = true;
        } else {
            out.push_str(line);
        }
    }
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_last_entry_wins() {
        let map =
            "\u{feff}# comment\r\n\r\nwork  A\r\nside\tB extra\r\nwork C\r\nbare\r\n  last D  ";
        assert_eq!(find(map, "work"), Some("C"));
        assert_eq!(find(map, "side"), Some("B"));
        assert_eq!(find(map, "last"), Some("D"));
        assert_eq!(find(map, "bare"), Some(""));
        assert_eq!(find(map, "nobody"), None);
    }

    #[test]
    fn find_never_matches_comments() {
        assert_eq!(find("#work A\n# work B\n", "#work"), None);
        assert_eq!(find("#work A\n", "work"), None);
    }

    #[test]
    fn set_appends_new_name() {
        assert_eq!(set("", "work", "A"), "work A\n");
        assert_eq!(set("# c\nside B\n", "work", "A"), "# c\nside B\nwork A\n");
        assert_eq!(set("side B", "work", "A"), "side B\nwork A\n");
    }

    #[test]
    fn set_replaces_only_the_last_entry() {
        let map = "work OLD1\n# keep\nside B\nwork   OLD2\nlast C\n";
        assert_eq!(
            set(map, "work", "NEW"),
            "work OLD1\n# keep\nside B\nwork NEW\nlast C\n"
        );
        assert_eq!(find(&set(map, "work", "NEW"), "work"), Some("NEW"));
    }

    #[test]
    fn names_in_first_appearance_order() {
        let map = "\u{feff}# c\nwork A\nside B\n\nwork C\nbare\n#off D\n";
        assert_eq!(names(map), ["work", "side", "bare"]);
        assert!(names("").is_empty());
    }

    #[test]
    fn set_replaces_entry_without_trailing_newline() {
        assert_eq!(set("side B\nwork OLD", "work", "NEW"), "side B\nwork NEW\n");
    }

    #[test]
    fn set_keeps_crlf_and_bom() {
        let map = "\u{feff}side B\r\n";
        assert_eq!(set(map, "work", "A"), "\u{feff}side B\r\nwork A\r\n");
        assert_eq!(set(map, "side", "C"), "\u{feff}side C\r\n");
        assert_eq!(set("\u{feff}", "work", "A"), "\u{feff}work A\n");
    }

    #[test]
    fn remove_drops_every_entry_for_name() {
        let map = "work A\n# work comment\nside B\nwork C";
        assert_eq!(
            remove(map, "work").as_deref(),
            Some("# work comment\nside B\n")
        );
        assert_eq!(remove(map, "nobody"), None);
        assert_eq!(
            remove("\u{feff}work A\r\nside B\r\n", "work").as_deref(),
            Some("\u{feff}side B\r\n")
        );
    }

    #[test]
    fn validation() {
        assert!(valid_name("work"));
        for name in ["", "#work", "a b", "a\tb"] {
            assert!(!valid_name(name), "{name:?}");
        }
        assert!(valid_token("github_pat_x"));
        for token in ["", "a b", "a\n"] {
            assert!(!valid_token(token), "{token:?}");
        }
    }
}
