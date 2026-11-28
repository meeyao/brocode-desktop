//! Paths as the server reports them, ready to show: either separator (`/` or `\`), drive
//! letters (`C:\`), UNC shares (`\\server\share`) and the home folder as `~`, whatever OS this
//! client runs on (Desktop's `getFilename`, `getDirectory` and `containsDirectory` in
//! `packages/util/src/path.ts`).

/// Both separators, for `split` and `trim_end_matches`.
pub const SEPARATORS: [char; 2] = ['/', '\\'];

/// The last segment: `acme-web` for `C:\repos\acme-web\`, `format.ts` for `src/util/format.ts`.
/// A root is its own name (`C:\`, `/`), a UNC share root its share (`share`).
pub fn basename(path: &str) -> &str {
    let path = trim(path);
    let root = root_len(path);
    if path.len() <= root {
        return if unc(path) { path[2..].rsplit(SEPARATORS).next().unwrap_or(path) } else { path };
    }
    path[root..].rsplit(SEPARATORS).next().unwrap_or(path)
}

/// Movie types the desktop file viewer and chat recognize (util/src/artifact.ts).
const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "m4v", "webm", "mov", "ogv", "mkv"];

pub fn is_video(path: &str) -> bool {
    basename(path).rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty() && VIDEO_EXTENSIONS.iter().any(|e| extension.eq_ignore_ascii_case(e))
    })
}

/// Cheap reject before parsing a reply a second time for standalone movie links.
pub fn mentions_video(source: &str) -> bool {
    source.match_indices('.').any(|(pos, _)| {
        let tail = &source.as_bytes()[pos + 1..];
        VIDEO_EXTENSIONS.iter().any(|e| tail.get(..e.len()).is_some_and(|t| t.eq_ignore_ascii_case(e.as_bytes())))
    })
}

/// The folder `path` is in, without a trailing separator except on a root: `src/util`,
/// `C:\repos`, `C:\`, `/`. Empty for a single segment or a root.
pub fn parent(path: &str) -> &str {
    let path = trim(path);
    let root = root_len(path);
    if path.len() <= root {
        return "";
    }
    match path[root..].rfind(SEPARATORS) {
        Some(i) => trim(&path[..root + i]),
        None => &path[..root],
    }
}

/// `path` split before its last segment, the folder part keeping its separator, for a muted
/// folder beside a file name: `("src/util/", "format.ts")`, `("", "README.md")`.
pub fn split(path: &str) -> (&str, &str) {
    let trimmed = trim(path);
    let name = basename(path);
    (&trimmed[..trimmed.len() - name.len()], name)
}

/// The last `n` segments, joined as written: `util/format.ts` for `C:\acme\src\util\format.ts` at 2.
pub fn tail(path: &str, n: usize) -> &str {
    let path = trim(path);
    let mut start = path.len();
    for _ in 0..n {
        match path[..start].rfind(SEPARATORS) {
            Some(i) => start = i,
            None => return path,
        }
    }
    &path[start + 1..]
}

/// `path` without trailing separators, a root keeping its own (`C:\`, `/`).
pub fn trim(path: &str) -> &str {
    let trimmed = path.trim_end_matches(SEPARATORS);
    let root = root_len(path);
    if trimmed.len() < root { &path[..root] } else { trimmed }
}

/// `path` with the home folder `home` shown as `~`: `~/code/acme`, `~\repos\acme-web`. Drive and
/// UNC paths compare case-insensitively.
pub fn tilde(path: &str, home: &str) -> String {
    let home = trim(home);
    let Some(head) = path.get(..home.len()).filter(|_| !home.is_empty() && home != "/") else {
        return path.to_owned();
    };
    let rest = &path[home.len()..];
    if same(head, home) && (rest.is_empty() || rest.starts_with(SEPARATORS)) { format!("~{rest}") } else { path.to_owned() }
}

/// The home folder `dir` sits in, as Desktop infers a server's (`inferHomeDirectory`,
/// `global-sync/bootstrap.ts:76-83`): `/Users/<you>`, `/home/<you>`, `/root` or
/// `C:\Users\<you>`, in `dir`'s own separators; empty when it's in none.
pub fn infer_home(dir: &str) -> String {
    let path = dir.replace('\\', "/");
    let parts: Vec<&str> = path.split('/').collect();
    let home = match parts.as_slice() {
        ["", "root", ..] => "/root".to_owned(),
        ["", top @ ("Users" | "home"), you, ..] if !you.is_empty() => format!("/{top}/{you}"),
        [drive, "Users", you, ..] if drive.len() == 2 && drive.ends_with(':') && drive.as_bytes()[0].is_ascii_alphabetic() && !you.is_empty() => format!("{drive}/Users/{you}"),
        _ => return String::new(),
    };
    if dir.contains('\\') { home.replace('/', "\\") } else { home }
}

/// Whether `path` starts at a root: `/`, `C:` or a UNC share.
pub fn is_absolute(path: &str) -> bool {
    root_len(path) > 0
}

/// The part of `path` inside `dir`, without a leading separator: `src/a.ts` for
/// `C:\acme\src/a.ts` in `C:\acme`. None outside it, and for `dir` itself.
pub fn strip_dir<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let dir = trim(dir);
    let rest = path.get(dir.len()..).filter(|_| !dir.is_empty() && same(&path[..dir.len()], dir))?;
    let inside = dir.ends_with(SEPARATORS) || rest.starts_with(SEPARATORS);
    Some(rest.trim_start_matches(SEPARATORS)).filter(|r| inside && !r.is_empty())
}

/// `a` and `b` name the same path: separators match each other, and Windows paths ignore case.
pub fn same(a: &str, b: &str) -> bool {
    let fold = windows(a) || windows(b);
    let sep = |c: u8| c == b'/' || c == b'\\';
    a.len() == b.len() && a.bytes().zip(b.bytes()).all(|(x, y)| x == y || (fold && ((sep(x) && sep(y)) || x.eq_ignore_ascii_case(&y))))
}

/// `child` is `parent` or inside it, whichever separator either uses. Drive and UNC paths
/// compare case-insensitively.
pub fn contains(parent: &str, child: &str) -> bool {
    let (parent, child) = (trim(parent), trim(child));
    !parent.is_empty() && (same(child, parent) || strip_dir(child, parent).is_some())
}

/// `path` under `directory` unless it's already absolute (`request.ts`'s `absolute`).
pub fn absolute(directory: &str, path: &str) -> String {
    if is_absolute(path) {
        return path.to_owned();
    }
    format!("{}/{path}", directory.trim_end_matches(SEPARATORS))
}

/// `encodeFilePath`: each segment percent-encoded as `encodeURIComponent` does, `/` kept, and
/// a Windows drive rooted (`/C:/…`), for a `file://` URI.
pub fn encode_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let drive = |s: &str| s.len() == 2 && s.as_bytes()[0].is_ascii_alphabetic() && s.as_bytes()[1] == b':';
    let rooted = if normalized.get(..2).is_some_and(drive) { format!("/{normalized}") } else { normalized };
    rooted
        .split('/')
        .enumerate()
        .map(|(i, seg)| if i == 1 && drive(seg) { seg.to_owned() } else { crate::client::encode_component(seg) })
        .collect::<Vec<String>>()
        .join("/")
}

/// A Windows path: a drive (`C:`) or a UNC share (`\\server`).
pub fn windows(path: &str) -> bool {
    drive(path) || unc(path)
}

fn drive(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn unc(path: &str) -> bool {
    path.len() > 2 && (path.starts_with("\\\\") || path.starts_with("//")) && !path[2..].starts_with(SEPARATORS)
}

/// How long the root is: `C:\` 3, `C:` 2, `/` 1, `\\server\share` its length, else 0.
fn root_len(path: &str) -> usize {
    if drive(path) {
        return if path[2..].starts_with(SEPARATORS) { 3 } else { 2 };
    }
    if unc(path) {
        let rest = &path[2..];
        let server = rest.find(SEPARATORS).unwrap_or(rest.len());
        let after = &rest[server..];
        let share = after.trim_start_matches(SEPARATORS);
        let skipped = after.len() - share.len();
        let end = share.find(SEPARATORS).unwrap_or(share.len());
        return 2 + server + skipped + end;
    }
    usize::from(path.starts_with(SEPARATORS))
}
