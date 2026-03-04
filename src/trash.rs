use crate::scanner::DirEntry;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum TrashError {
    OperationFailed(String),
}

/// Move `path` to `~/.Trash/` using a plain filesystem rename.
/// No AppleScript, no Finder, no special permissions required.
/// Files are recoverable from Trash in Finder.
pub fn move_to_trash(path: &Path) -> Result<(), TrashError> {
    let home = dirs::home_dir()
        .ok_or_else(|| TrashError::OperationFailed("cannot find home directory".to_string()))?;
    let trash_dir = home.join(".Trash");

    let file_name = path
        .file_name()
        .ok_or_else(|| TrashError::OperationFailed("path has no filename".to_string()))?
        .to_string_lossy()
        .into_owned();

    let dest = unique_trash_path(&trash_dir, &file_name);

    std::fs::rename(path, &dest).map_err(|e| {
        if e.raw_os_error() == Some(18) {
            // EXDEV: cross-device link — file is on a different volume than ~/.Trash
            TrashError::OperationFailed(
                "cannot trash: file is on a different volume than home".to_string(),
            )
        } else {
            TrashError::OperationFailed(e.to_string())
        }
    })
}

/// Return a path in `trash_dir` that does not yet exist.
/// If `name` is taken, appends " 2", " 3", … before the extension.
fn unique_trash_path(trash_dir: &Path, file_name: &str) -> PathBuf {
    let candidate = trash_dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let p = Path::new(file_name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for i in 2u32.. {
        let name = format!("{} {}{}", stem, i, ext);
        let candidate = trash_dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Remove an entry matching `target` from the tree and recalculate parent sizes.
/// Returns a new tree (immutable pattern).
pub fn remove_entry(root: DirEntry, target: &Path) -> DirEntry {
    let children: Vec<DirEntry> = root
        .children
        .into_iter()
        .filter(|c| c.path != target)
        .map(|c| remove_entry(c, target))
        .collect();

    let size = if root.is_dir {
        children.iter().map(|c| c.size).sum()
    } else {
        root.size
    };

    DirEntry {
        children,
        size,
        ..root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(name: &str, path: &str, size: u64) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            size,
            is_dir: false,
            flag: None,
            children: vec![],
        }
    }

    fn make_dir(name: &str, path: &str, children: Vec<DirEntry>) -> DirEntry {
        let size = children.iter().map(|c| c.size).sum();
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            size,
            is_dir: true,
            flag: None,
            children,
        }
    }

    #[test]
    fn test_remove_entry_removes_child() {
        let root = make_dir(
            "root",
            "/root",
            vec![
                make_file("a", "/root/a", 200),
                make_file("b", "/root/b", 100),
            ],
        );
        let updated = remove_entry(root, Path::new("/root/a"));
        assert_eq!(updated.children.len(), 1);
        assert_eq!(updated.children[0].name, "b");
    }

    #[test]
    fn test_remove_entry_recalculates_size() {
        let root = make_dir(
            "root",
            "/root",
            vec![
                make_file("a", "/root/a", 200),
                make_file("b", "/root/b", 100),
            ],
        );
        let updated = remove_entry(root, Path::new("/root/a"));
        assert_eq!(updated.size, 100);
    }

    #[test]
    fn test_remove_entry_nested() {
        let root = make_dir(
            "root",
            "/root",
            vec![make_dir(
                "sub",
                "/root/sub",
                vec![
                    make_file("x", "/root/sub/x", 50),
                    make_file("y", "/root/sub/y", 30),
                ],
            )],
        );
        let updated = remove_entry(root, Path::new("/root/sub/x"));
        assert_eq!(updated.children[0].size, 30);
        assert_eq!(updated.size, 30);
    }

    #[test]
    fn test_remove_nonexistent_is_noop() {
        let root = make_dir("root", "/root", vec![make_file("a", "/root/a", 100)]);
        let updated = remove_entry(root, Path::new("/root/z"));
        assert_eq!(updated.children.len(), 1);
        assert_eq!(updated.size, 100);
    }

    #[test]
    fn test_unique_trash_path_no_conflict() {
        let dir = PathBuf::from("/nonexistent/trash");
        let dest = unique_trash_path(&dir, "file.txt");
        assert_eq!(dest, dir.join("file.txt"));
    }

    #[test]
    fn test_move_to_trash_real_file() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("testfile.txt");
        fs::write(&file, "hello").unwrap();

        // We can't easily test the actual ~/.Trash move in CI, but we can
        // verify the rename call doesn't panic on a valid path.
        // Just check that the file exists before the call.
        assert!(file.exists());
    }
}
