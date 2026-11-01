use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    size: u64,
    sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Issue {
    path: String,
    reason: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclusions: Vec<String>,
    version: u32,
    complete: bool,
    files: Vec<Entry>,
    issues: Vec<Issue>,
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err("not a regular file".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut size = 0;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hasher.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    if size != before.len()
        || after.len() != before.len()
        || after.modified()? != before.modified()?
    {
        return Err("file changed during scan".into());
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn walk(
    root: &Path,
    dir: &Path,
    manifest: &mut Manifest,
    remaining: &mut usize,
    stopped: &mut bool,
) {
    let relative = |p: &Path| {
        p.strip_prefix(root)
            .unwrap_or(p)
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            manifest.issues.push(Issue {
                path: relative(dir),
                reason: "unreadable_directory".into(),
            });
            return;
        }
    };
    for item in entries {
        if *stopped {
            return;
        }
        if *remaining == 0 {
            manifest.issues.push(Issue {
                path: String::new(),
                reason: "entry_limit".into(),
            });
            *stopped = true;
            return;
        }
        *remaining -= 1;
        let item = match item {
            Ok(item) => item,
            Err(_) => {
                manifest.issues.push(Issue {
                    path: relative(dir),
                    reason: "unreadable_directory_entry".into(),
                });
                continue;
            }
        };
        let path = item.path();
        let rel = relative(&path);
        if manifest
            .exclusions
            .iter()
            .any(|rule| rel == *rule || rel.starts_with(&format!("{rule}/")))
        {
            continue;
        }
        let reason = if path
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.to_str())
            .is_none()
        {
            Some("non_utf8_path")
        } else if rel.contains('\\') || rel.contains(':') {
            Some("unsupported_path")
        } else if path.strip_prefix(root).unwrap().components().count() > 128 {
            Some("depth_limit")
        } else {
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => Some("skipped_symlink"),
                Ok(meta) if meta.is_dir() => {
                    walk(root, &path, manifest, remaining, stopped);
                    None
                }
                Ok(meta) if meta.is_file() => match hash_file(&path) {
                    Ok((size, sha256)) => {
                        manifest.files.push(Entry {
                            path: rel.clone(),
                            size,
                            sha256,
                        });
                        None
                    }
                    Err(_) => Some("unreadable_or_changed_file"),
                },
                Ok(_) => Some("skipped_special_file"),
                Err(_) => Some("unreadable_metadata"),
            }
        };
        if let Some(reason) = reason {
            manifest.issues.push(Issue {
                path: rel,
                reason: reason.into(),
            });
        }
    }
}

#[cfg(test)]
fn scan(root: &Path) -> Result<Manifest> {
    scan_limited(root, 100_000)
}

#[cfg(test)]
fn scan_limited(root: &Path, limit: usize) -> Result<Manifest> {
    scan_policy(root, limit, vec![])
}

fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(':'))
}
fn scan_policy(root: &Path, limit: usize, mut exclusions: Vec<String>) -> Result<Manifest> {
    exclusions.sort();
    exclusions.dedup();
    if exclusions.len() > 100
        || exclusions
            .iter()
            .any(|p| !valid_relative(p) || p.len() > 4096)
    {
        return Err(
            "exclusions require at most 100 exact relative paths, up to 4096 bytes each".into(),
        );
    }
    if limit == 0 || limit > 100_000 {
        return Err("entry limit must be 1 to 100000".into());
    }
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("root must be a real directory, not a symlink".into());
    }
    let mut result = Manifest {
        version: if exclusions.is_empty() { 1 } else { 2 },
        exclusions,
        complete: true,
        files: vec![],
        issues: vec![],
    };
    walk(root, root, &mut result, &mut { limit }, &mut false);
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    result
        .issues
        .sort_by(|a, b| (&a.path, &a.reason).cmp(&(&b.path, &b.reason)));
    result.complete = result.issues.is_empty();
    Ok(result)
}

fn validate(manifest: &Manifest) -> Result<()> {
    if !matches!(manifest.version, 1 | 2)
        || (manifest.version == 1 && !manifest.exclusions.is_empty())
        || manifest.complete != manifest.issues.is_empty()
    {
        return Err("invalid manifest version or completeness".into());
    }
    if manifest.exclusions.len() > 100
        || manifest
            .exclusions
            .iter()
            .any(|p| !valid_relative(p) || p.len() > 4096)
        || manifest
            .exclusions
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err("invalid exclusion policy".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &manifest.files {
        if manifest
            .exclusions
            .iter()
            .any(|rule| entry.path == *rule || entry.path.starts_with(&format!("{rule}/")))
        {
            return Err("manifest contains an excluded path".into());
        }
        if entry.path.is_empty()
            || entry.path.contains('\\')
            || entry
                .path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".." || p.contains(':'))
            || !seen.insert(&entry.path)
        {
            return Err("invalid or duplicate relative path".into());
        }
        if entry.sha256.len() != 64
            || !entry
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid SHA-256 digest".into());
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<Manifest> {
    let mut data = Vec::new();
    File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 16 * 1024 * 1024 {
        return Err("manifest exceeds 16 MiB".into());
    }
    let manifest = serde_json::from_slice(&data)?;
    validate(&manifest)?;
    Ok(manifest)
}

fn compare(before: &Manifest, after: &Manifest) -> Result<serde_json::Value> {
    validate(before)?;
    validate(after)?;
    if before.exclusions != after.exclusions {
        return Err("cannot compare different exclusion policies".into());
    }
    if !before.complete || !after.complete {
        return Err("cannot compare incomplete manifests; inspect scan issues".into());
    }
    let old: BTreeMap<_, _> = before.files.iter().map(|e| (&e.path, e)).collect();
    let new: BTreeMap<_, _> = after.files.iter().map(|e| (&e.path, e)).collect();
    let mut changes = Vec::new();
    let keys: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).collect();
    for path in keys {
        let kind = match (old.get(path), new.get(path)) {
            (None, Some(_)) => Some("added"),
            (Some(_), None) => Some("removed"),
            (Some(a), Some(b)) if a.size != b.size || a.sha256 != b.sha256 => Some("changed"),
            _ => None,
        };
        if let Some(kind) = kind {
            changes.push(serde_json::json!({"path":path,"kind":kind}));
        }
    }
    Ok(
        serde_json::json!({"version":1,"match":changes.is_empty(),"changes":changes,"exclusions":before.exclusions}),
    )
}

// Stage in the destination directory so publication stays on one filesystem.
// persist_noclobber never replaces an existing output, including a symlink.
fn publish_with(output: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::Builder::new()
        .prefix(".treeledger-")
        .tempfile_in(parent)?;
    write(staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(output)?;
    Ok(())
}

struct LimitedWriter<W> {
    inner: W,
    remaining: usize,
}
impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other(
                "manifest exceeds 16 MiB; nothing published",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn publish_manifest(output: &Path, manifest: &Manifest, limit: usize) -> Result<()> {
    publish_with(output, |file| {
        let writer = LimitedWriter {
            inner: file,
            remaining: limit,
        };
        serde_json::to_writer_pretty(writer, manifest)?;
        Ok(())
    })
}

fn run(args: &[String]) -> Result<i32> {
    match args {
        [command, root, output, options @ ..] if command == "scan" => {
            let mut limit = 100_000;
            let mut exclusions = Vec::new();
            let mut seen_limit = false;
            for pair in options.chunks(2) {
                match pair {
                    [flag, value] if flag == "--max-entries" && !seen_limit => { limit = value.parse::<usize>()?; seen_limit = true; },
                    [flag, value] if flag == "--exclude" => exclusions.push(value.clone()),
                    _ => return Err("expected --max-entries N or --exclude RELATIVE_PATH".into()),
                }
            }
            let root_path = Path::new(root);
            let output = Path::new(output);
            let parent = output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            if parent
                .canonicalize()?
                .starts_with(root_path.canonicalize()?)
            {
                return Err("manifest output must be outside scan root".into());
            }
            let manifest = scan_policy(root_path, limit, exclusions)?;
            validate(&manifest)?;
            publish_manifest(output, &manifest, 16 * 1024 * 1024)?;
            println!(
                "{}",
                serde_json::json!({"complete":manifest.complete,"files":manifest.files.len(),"issues":manifest.issues.len()})
            );
            Ok(if manifest.complete { 0 } else { 1 })
        }
        [command, before, after] if command == "compare" => {
            let report = compare(&load(Path::new(before))?, &load(Path::new(after))?)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(if report["match"] == true { 0 } else { 1 })
        }
        _ => Err(
            "usage: treeledger scan ROOT MANIFEST.json [--max-entries N] [--exclude PATH] | treeledger compare BEFORE.json AFTER.json"
                .into(),
        ),
    }
}

fn main() {
    let code = match run(&env::args().skip(1).collect::<Vec<_>>()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", serde_json::json!({"error":error.to_string()}));
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusions_are_visible_scoped_and_must_match() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("cache")).unwrap();
        fs::write(temp.path().join("cache/a"), b"a").unwrap();
        fs::write(temp.path().join("cache-other"), b"b").unwrap();
        let scoped = scan_policy(temp.path(), 100, vec!["cache".into()]).unwrap();
        assert!(scoped.complete);
        assert_eq!(scoped.version, 2);
        assert_eq!(scoped.files.len(), 1);
        assert_eq!(scoped.files[0].path, "cache-other");
        assert!(compare(&scoped, &scan(temp.path()).unwrap()).is_err());
        assert_eq!(compare(&scoped, &scoped).unwrap()["match"], true);
        assert!(scan_policy(temp.path(), 100, vec!["../cache".into()]).is_err());
        let encoded = serde_json::to_vec(&scoped).unwrap();
        validate(&serde_json::from_slice::<Manifest>(&encoded).unwrap()).unwrap();
    }

    #[test]
    fn oversized_serialization_never_publishes_or_leaves_staging() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a"), b"a").unwrap();
        let manifest = scan(&root).unwrap();
        let output = temp.path().join("out.json");
        let size = serde_json::to_vec_pretty(&manifest).unwrap().len();
        assert!(publish_manifest(&output, &manifest, size - 1).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        publish_manifest(&output, &manifest, size).unwrap();
        assert_eq!(load(&output).unwrap().files, manifest.files);
    }

    #[test]
    fn traversal_budget_marks_partial_results_and_counts_directories() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("nested/a"), b"a").unwrap();
        fs::write(temp.path().join("nested/b"), b"b").unwrap();
        let limited = scan_limited(temp.path(), 2).unwrap();
        assert!(!limited.complete);
        assert_eq!(limited.files.len(), 1);
        assert_eq!(limited.issues.len(), 1);
        assert_eq!(limited.issues[0].reason, "entry_limit");
        assert!(compare(&limited, &limited).is_err());
        assert!(scan_limited(temp.path(), 3).unwrap().complete);
        assert!(scan_limited(temp.path(), 0).is_err());
        assert!(scan_limited(temp.path(), 100_001).is_err());
    }

    #[test]
    fn publication_failure_cleans_staging_and_preserves_destination() {
        let temp = tempfile::tempdir().unwrap();
        let out = temp.path().join("manifest.json");
        let result = publish_with(&out, |file| {
            file.write_all(b"partial")?;
            assert!(!out.exists());
            Err("injected write failure".into())
        });
        assert!(result.is_err());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        fs::write(&out, b"existing").unwrap();
        assert!(
            publish_with(&out, |f| {
                f.write_all(b"new")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&out).unwrap(), b"existing");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn destination_created_during_write_is_not_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let out = temp.path().join("manifest.json");
        assert!(
            publish_with(&out, |f| {
                f.write_all(b"candidate")?;
                fs::write(&out, b"winner")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(out).unwrap(), b"winner");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn hash_known_bytes_and_empty() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("a");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(
            hash_file(&file).unwrap(),
            (
                3,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()
            )
        );
        fs::write(&file, b"").unwrap();
        assert_eq!(hash_file(&file).unwrap().0, 0);
    }
    #[test]
    fn scan_sorted_nested_and_repeatable() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("z"), b"\x00\xff").unwrap();
        fs::write(temp.path().join("nested/a"), b"hello").unwrap();
        let a = scan(temp.path()).unwrap();
        let b = scan(temp.path()).unwrap();
        assert!(a.complete);
        assert_eq!(a.files, b.files);
        assert_eq!(a.files[0].path, "nested/a");
    }
    #[test]
    fn compare_added_removed_same_size_change() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a"), b"abc").unwrap();
        fs::write(temp.path().join("gone"), b"x").unwrap();
        let before = scan(temp.path()).unwrap();
        fs::write(temp.path().join("a"), b"xyz").unwrap();
        fs::remove_file(temp.path().join("gone")).unwrap();
        fs::write(temp.path().join("new"), b"y").unwrap();
        let report = compare(&before, &scan(temp.path()).unwrap()).unwrap();
        assert_eq!(report["changes"].as_array().unwrap().len(), 3);
        assert_eq!(report["changes"][0]["kind"], "changed");
        assert_eq!(compare(&before, &before).unwrap()["match"], true);
    }
    #[test]
    fn empty_and_missing_root() {
        let temp = tempfile::tempdir().unwrap();
        assert!(scan(temp.path()).unwrap().files.is_empty());
        assert!(scan(&temp.path().join("missing")).is_err());
    }
    #[test]
    fn invalid_manifest_rejected() {
        let mut m = Manifest {
            version: 1,
            exclusions: vec![],
            complete: true,
            files: vec![],
            issues: vec![],
        };
        for path in ["../x", "/x", "a//b", "a\\b"] {
            m.files = vec![Entry {
                path: path.into(),
                size: 0,
                sha256: "a".repeat(64),
            }];
            assert!(validate(&m).is_err());
        }
        m.files.clear();
        m.complete = false;
        assert!(validate(&m).is_err());
        m.issues.push(Issue {
            path: "x".into(),
            reason: "unreadable".into(),
        });
        assert!(compare(&m, &m).is_err());
    }
    #[test]
    fn output_refuses_overwrite_and_inside_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let out = temp.path().join("out.json");
        let args = vec![
            "scan".into(),
            root.to_str().unwrap().into(),
            out.to_str().unwrap().into(),
        ];
        assert_eq!(run(&args).unwrap(), 0);
        assert!(run(&args).is_err());
        assert!(load(&out).unwrap().complete);
        assert!(
            run(&[
                "scan".into(),
                root.to_str().unwrap().into(),
                root.join("out.json").to_str().unwrap().into()
            ])
            .is_err()
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlink_skipped_and_comparison_refused() {
        let temp = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("missing", temp.path().join("link")).unwrap();
        let m = scan(temp.path()).unwrap();
        assert!(!m.complete);
        assert_eq!(m.issues[0].reason, "skipped_symlink");
        assert!(m.files.is_empty());
        assert!(compare(&m, &m).is_err());
    }
}
