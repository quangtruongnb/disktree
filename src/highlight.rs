use crate::scanner::{DirEntry, Flag};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn apply_flags(entry: DirEntry, home_dir: &Path) -> DirEntry {
    let cache_dir = home_dir.join("Library/Caches");
    let brew_cache = resolve_brew_cache(home_dir);

    apply_flags_inner(entry, &cache_dir, brew_cache.as_deref())
}

fn apply_flags_inner(entry: DirEntry, cache_dir: &Path, brew_cache: Option<&Path>) -> DirEntry {
    let flag = detect_flag(&entry.path, cache_dir, brew_cache);
    let children = entry
        .children
        .into_iter()
        .map(|child| apply_flags_inner(child, cache_dir, brew_cache))
        .collect();
    DirEntry {
        flag,
        children,
        ..entry
    }
}

fn detect_flag(path: &Path, cache_dir: &Path, brew_cache: Option<&Path>) -> Option<Flag> {
    if path.starts_with(cache_dir) && path != cache_dir {
        return Some(Flag::Cache);
    }
    if let Some(brew) = brew_cache {
        if path.starts_with(brew) && path != brew {
            return Some(Flag::Brew);
        }
    }
    None
}

fn resolve_brew_cache(home_dir: &Path) -> Option<PathBuf> {
    Command::new("brew")
        .arg("--cache")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim()))
        .or_else(|| Some(home_dir.join(".cache/homebrew")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_dir(path: &str) -> DirEntry {
        DirEntry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: PathBuf::from(path),
            size: 0,
            is_dir: true,
            flag: None,
            children: vec![],
        }
    }

    #[test]
    fn test_cache_flag_applied_to_child() {
        let home = PathBuf::from("/Users/test");
        let entry = make_dir("/Users/test/Library/Caches/com.apple.Safari");
        let flagged = apply_flags(entry, &home);
        assert_eq!(flagged.flag, Some(Flag::Cache));
    }

    #[test]
    fn test_cache_dir_itself_not_flagged() {
        let home = PathBuf::from("/Users/test");
        let entry = make_dir("/Users/test/Library/Caches");
        let flagged = apply_flags(entry, &home);
        assert_eq!(flagged.flag, None);
    }

    #[test]
    fn test_no_flag_for_unrelated_dir() {
        let home = PathBuf::from("/Users/test");
        let entry = make_dir("/Users/test/Documents");
        let flagged = apply_flags(entry, &home);
        assert_eq!(flagged.flag, None);
    }

    #[test]
    fn test_flags_applied_recursively() {
        let home = PathBuf::from("/Users/test");
        let parent = DirEntry {
            name: "Library".to_string(),
            path: PathBuf::from("/Users/test/Library"),
            size: 0,
            is_dir: true,
            flag: None,
            children: vec![make_dir("/Users/test/Library/Caches/Safari")],
        };
        let flagged = apply_flags(parent, &home);
        assert_eq!(flagged.flag, None);
        assert_eq!(flagged.children[0].flag, Some(Flag::Cache));
    }

    #[test]
    fn test_brew_cache_fallback_path() {
        let home = PathBuf::from("/Users/test");
        let result = resolve_brew_cache(&home);
        assert!(result.is_some());
    }
}
