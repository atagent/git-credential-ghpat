//! The credentials table: one `pattern token` entry per line, separated by
//! whitespace. Blank lines and lines starting with `#` are ignored.
//!
//! A pattern is `[profile@][owner[/repo]]`. The profile is git's username,
//! and the owner and repo come from git's path. A part that is left out
//! matches anything, so `@` matches every request. The last matching entry
//! wins.
//!
//! The management functions treat a pattern as a plain name, compared as
//! written. Edits keep every other line as it is, including comments, order,
//! CRLF line endings, and a leading UTF-8 BOM.

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

/// The token on an entry line, or "" if there is none.
fn token(line: &str) -> &str {
    line.split_whitespace().nth(1).unwrap_or("")
}

/// A parsed pattern. A part that is None matches anything.
#[derive(Debug, PartialEq)]
struct Pattern<'a> {
    profile: Option<&'a str>,
    owner: Option<&'a str>,
    repo: Option<&'a str>,
}

/// Parse `[profile@][owner[/repo]]`, or return None if it isn't one.
/// An empty profile before the `@` matches any profile.
fn parse_pattern(pattern: &str) -> Option<Pattern<'_>> {
    let (profile, place) = match pattern.split_once('@') {
        Some((profile, place)) => (Some(profile).filter(|p| !p.is_empty()), place),
        None if pattern.is_empty() => return None,
        None => (None, pattern),
    };
    if place.contains('@') {
        return None;
    }
    let (owner, repo) = match place.split_once('/') {
        None => (Some(place).filter(|o| !o.is_empty()), None),
        Some((owner, repo)) if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') => {
            (Some(owner), Some(repo))
        }
        Some(_) => return None,
    };
    Some(Pattern {
        profile,
        owner,
        repo,
    })
}

impl Pattern<'_> {
    /// Whether the pattern matches a request. The profile is compared
    /// exactly; the owner and repo ignore ASCII case, as GitHub does.
    fn matches(&self, profile: Option<&str>, owner: Option<&str>, repo: Option<&str>) -> bool {
        let same = |want: Option<&str>, got: Option<&str>| {
            want.is_none_or(|want| got.is_some_and(|got| got.eq_ignore_ascii_case(want)))
        };
        self.profile.is_none_or(|want| profile == Some(want))
            && same(self.owner, owner)
            && same(self.repo, repo)
    }
}

/// The owner and repo from git's path, e.g. `owner/repo.git/info/lfs`.
/// A trailing `.git` is dropped from the repo.
fn split_path(path: &str) -> (Option<&str>, Option<&str>) {
    let mut parts = path.split('/');
    let owner = parts.next().filter(|o| !o.is_empty());
    let repo = parts
        .next()
        .map(|r| r.strip_suffix(".git").unwrap_or(r))
        .filter(|r| !r.is_empty());
    (owner, repo)
}

/// A pattern must be one word that can't be read as a comment and parses as
/// `[profile@][owner[/repo]]`.
pub fn valid_pattern(pattern: &str) -> bool {
    !pattern.starts_with('#')
        && !pattern.contains(char::is_whitespace)
        && parse_pattern(pattern).is_some()
}

/// A token must be one non-empty word.
pub fn valid_token(token: &str) -> bool {
    !token.is_empty() && !token.contains(char::is_whitespace)
}

/// Return the token listed for name exactly as written, or None if name is
/// not listed. The last entry wins; an entry without a token gives "".
pub fn find<'a>(contents: &'a str, name: &str) -> Option<&'a str> {
    let (_, body) = split_bom(contents);
    body.lines()
        .rfind(|line| key(line) == Some(name))
        .map(token)
}

/// Return the pattern and token of the last entry that matches a request
/// with git's username and path, or None if no entry matches. Invalid
/// patterns never match. An entry without a token gives "".
pub fn lookup<'a>(
    contents: &'a str,
    username: Option<&str>,
    path: Option<&str>,
) -> Option<(&'a str, &'a str)> {
    let username = username.filter(|u| !u.is_empty());
    let (owner, repo) = split_path(path.unwrap_or(""));
    let (_, body) = split_bom(contents);
    body.lines()
        .filter_map(|line| Some((key(line)?, token(line))))
        .rfind(|(pattern, _)| {
            parse_pattern(pattern).is_some_and(|p| p.matches(username, owner, repo))
        })
}

/// The listed patterns that don't parse, in order.
pub fn invalid(contents: &str) -> Vec<&str> {
    let (_, body) = split_bom(contents);
    body.lines()
        .filter_map(key)
        .filter(|pattern| parse_pattern(pattern).is_none())
        .collect()
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
    fn parse_valid_patterns() {
        let pat = |profile, owner, repo| {
            Some(Pattern {
                profile,
                owner,
                repo,
            })
        };
        assert_eq!(parse_pattern("@"), pat(None, None, None));
        assert_eq!(parse_pattern("work@"), pat(Some("work"), None, None));
        assert_eq!(parse_pattern("acme"), pat(None, Some("acme"), None));
        assert_eq!(
            parse_pattern("acme/api"),
            pat(None, Some("acme"), Some("api"))
        );
        assert_eq!(parse_pattern("@acme"), pat(None, Some("acme"), None));
        assert_eq!(
            parse_pattern("@acme/api"),
            pat(None, Some("acme"), Some("api"))
        );
        assert_eq!(parse_pattern("w@acme"), pat(Some("w"), Some("acme"), None));
        assert_eq!(
            parse_pattern("w@acme/api"),
            pat(Some("w"), Some("acme"), Some("api"))
        );
    }

    #[test]
    fn parse_invalid_patterns() {
        for pattern in [
            "", "/api", "w@/api", "acme/", "@acme/", "/", "a/b/c", "a@b@c", "@@",
        ] {
            assert_eq!(parse_pattern(pattern), None, "{pattern:?}");
        }
    }

    const RULES: &str = "\
@ ANY
work@ WORK
acme ACME
acme/api ACME_API
side@acme SIDE_ACME
bad/ BAD
";

    fn token_for(username: Option<&str>, path: Option<&str>) -> Option<&'static str> {
        lookup(RULES, username, path).map(|(_, token)| token)
    }

    #[test]
    fn lookup_last_match_wins() {
        assert_eq!(token_for(None, None), Some("ANY"));
        assert_eq!(token_for(Some(""), Some("")), Some("ANY"));
        assert_eq!(token_for(Some("work"), None), Some("WORK"));
        assert_eq!(token_for(Some("other"), Some("x/y")), Some("ANY"));
        assert_eq!(token_for(Some("work"), Some("acme/web")), Some("ACME"));
        assert_eq!(token_for(None, Some("acme/api")), Some("ACME_API"));
        assert_eq!(token_for(Some("side"), Some("acme/api")), Some("SIDE_ACME"));
        assert_eq!(
            lookup(RULES, Some("side"), Some("acme/api")),
            Some(("side@acme", "SIDE_ACME"))
        );
    }

    #[test]
    fn lookup_owner_and_repo() {
        for path in ["acme/api", "ACME/Api.git", "acme/api.git/info/lfs"] {
            assert_eq!(token_for(None, Some(path)), Some("ACME_API"), "{path}");
        }
        for path in ["acme", "acme/", "acme/api2", "acme/.git"] {
            assert_eq!(token_for(None, Some(path)), Some("ACME"), "{path}");
        }
        // Without a path, rules with an owner never match.
        let rules = "acme A\nacme/api B\n";
        assert_eq!(lookup(rules, Some("work"), None), None);
        assert_eq!(lookup(rules, Some("work"), Some("")), None);
        assert_eq!(lookup(rules, None, Some("/api")), None);
    }

    #[test]
    fn lookup_profile_is_exact() {
        let rules = "work@ A\n";
        assert_eq!(lookup(rules, Some("Work"), None), None);
        assert_eq!(lookup(rules, None, Some("work/x")), None);
        assert_eq!(
            lookup(rules, Some("work"), Some("x/y")),
            Some(("work@", "A"))
        );
    }

    #[test]
    fn lookup_skips_invalid_and_bare_entries() {
        let rules = "\u{feff}@ A\r\nbad/ B\r\n# @ C\r\nwork@\r\n";
        assert_eq!(lookup(rules, None, None), Some(("@", "A")));
        assert_eq!(lookup(rules, Some("work"), None), Some(("work@", "")));
        assert_eq!(lookup("", None, None), None);
    }

    #[test]
    fn invalid_lists_unparsable_patterns() {
        assert_eq!(invalid(RULES), ["bad/"]);
        assert_eq!(
            invalid("\u{feff}/x A\n# a/ B\n@ C\na@b@c\n"),
            ["/x", "a@b@c"]
        );
        assert!(invalid("").is_empty());
    }

    #[test]
    fn validation() {
        for pattern in ["work", "work@", "@", "@acme/api"] {
            assert!(valid_pattern(pattern), "{pattern:?}");
        }
        for pattern in ["", "#work", "a b", "a\tb", "acme/", "#@"] {
            assert!(!valid_pattern(pattern), "{pattern:?}");
        }
        assert!(valid_token("github_pat_x"));
        for token in ["", "a b", "a\n"] {
            assert!(!valid_token(token), "{token:?}");
        }
    }
}
