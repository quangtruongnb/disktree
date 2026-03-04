use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq)]
pub enum Flag {
    Cache,
    Brew,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<DirEntry>,
    pub flag: Option<Flag>,
}

pub struct ScanResult {
    pub root: DirEntry,
    pub skipped_count: usize,
}

pub fn scan_directory(path: &Path) -> ScanResult {
    // Single pass: collect entries with actual allocated disk size.
    //
    // We use st_blocks * 512 instead of st_size to get real disk usage:
    //   - Sparse files (e.g. Docker .raw images) report a huge st_size but
    //     only small st_blocks — matching what `du` reports.
    //   - We deduplicate by (dev, ino) to avoid counting hardlinked files
    //     multiple times (macOS uses hardlinks extensively).
    let mut raw_entries: Vec<(PathBuf, bool, u64)> = Vec::new();
    let mut seen_dev_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut skipped_count = 0;

    for result in WalkDir::new(path).follow_links(false) {
        match result {
            Ok(entry) => {
                let is_dir = entry.file_type().is_dir();
                let is_symlink = entry.file_type().is_symlink();

                let size = if is_symlink || is_dir {
                    // Symlinks: negligible size.
                    // Directories: own block cost is typically 0 on APFS; we
                    // accumulate size from children instead.
                    0
                } else {
                    match entry.metadata() {
                        Ok(meta) => {
                            let dev_ino = (meta.dev(), meta.ino());
                            if seen_dev_inodes.insert(dev_ino) {
                                // st_blocks is in 512-byte units
                                meta.blocks() * 512
                            } else {
                                // Hardlink: inode already counted via another path
                                0
                            }
                        }
                        Err(_) => {
                            skipped_count += 1;
                            0
                        }
                    }
                };

                raw_entries.push((entry.path().to_path_buf(), is_dir, size));
            }
            Err(_) => {
                skipped_count += 1;
            }
        }
    }

    // Build HashMap of path -> DirEntry with leaf sizes only
    let mut map: HashMap<PathBuf, DirEntry> = HashMap::with_capacity(raw_entries.len());
    for (entry_path, is_dir, size) in &raw_entries {
        let name = entry_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| entry_path.to_string_lossy().to_string());

        map.insert(
            entry_path.clone(),
            DirEntry {
                name,
                path: entry_path.clone(),
                size: *size,
                is_dir: *is_dir,
                children: vec![],
                flag: None,
            },
        );
    }

    // Build tree bottom-up: sort deepest paths first so children are folded
    // into parents before parents are folded into grandparents.
    let mut sorted_paths: Vec<PathBuf> = raw_entries.iter().map(|(p, _, _)| p.clone()).collect();
    sorted_paths.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

    for entry_path in &sorted_paths {
        if let Some(parent_path) = entry_path.parent() {
            if map.contains_key(parent_path) {
                if let Some(entry) = map.remove(entry_path) {
                    let child_size = entry.size;
                    if let Some(parent) = map.get_mut(parent_path) {
                        parent.size += child_size;
                        parent.children.push(entry);
                    }
                }
            }
        }
    }

    let mut root = map.remove(path).unwrap_or_else(|| DirEntry {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string()),
        path: path.to_path_buf(),
        size: 0,
        is_dir: true,
        children: vec![],
        flag: None,
    });

    sort_children_by_size(&mut root);

    ScanResult {
        root,
        skipped_count,
    }
}

pub fn sort_children_by_size(entry: &mut DirEntry) {
    entry.children.sort_by(|a, b| b.size.cmp(&a.size));
    for child in &mut entry.children {
        sort_children_by_size(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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

    fn make_dir(name: &str, path: &str, size: u64, children: Vec<DirEntry>) -> DirEntry {
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
    fn test_sort_children_by_size_descending() {
        let mut root = make_dir(
            "root",
            "/root",
            300,
            vec![
                make_file("small", "/root/small", 50),
                make_file("large", "/root/large", 200),
                make_file("medium", "/root/medium", 50),
            ],
        );
        sort_children_by_size(&mut root);
        assert_eq!(root.children[0].name, "large");
        assert_eq!(root.children[0].size, 200);
    }

    #[test]
    fn test_sort_children_recursively() {
        let mut root = make_dir(
            "root",
            "/root",
            300,
            vec![make_dir(
                "sub",
                "/root/sub",
                150,
                vec![
                    make_file("a", "/root/sub/a", 30),
                    make_file("b", "/root/sub/b", 120),
                ],
            )],
        );
        sort_children_by_size(&mut root);
        assert_eq!(root.children[0].children[0].name, "b");
    }

    #[test]
    fn test_sort_empty_children() {
        let mut entry = make_file("file", "/file", 100);
        sort_children_by_size(&mut entry);
        assert!(entry.children.is_empty());
    }

    #[test]
    fn test_scan_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let result = scan_directory(tmp.path());
        assert_eq!(result.root.size, 0);
        assert!(result.root.children.is_empty());
        assert_eq!(result.skipped_count, 0);
    }

    #[test]
    fn test_scan_nonzero_file_sizes() {
        let tmp = TempDir::new().unwrap();
        // Large enough to guarantee non-zero block allocation on any filesystem
        fs::write(tmp.path().join("a.txt"), "x".repeat(8192)).unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.txt"), "y".repeat(16384)).unwrap();

        let result = scan_directory(tmp.path());
        // sub contains b.txt which is larger than a.txt
        assert!(result.root.size > 0);
        assert_eq!(result.root.children.len(), 2);
        assert_eq!(result.root.children[0].name, "sub");
        assert!(result.root.children[0].size > result.root.children[1].size);
    }

    #[test]
    fn test_scan_size_accumulation() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a/b")).unwrap();
        let content = "z".repeat(8192);
        fs::write(tmp.path().join("a/b/file.txt"), &content).unwrap();

        let result = scan_directory(tmp.path());
        // Root size should equal "a" size should equal actual allocated blocks
        let a = &result.root.children[0];
        assert_eq!(a.name, "a");
        assert_eq!(result.root.size, a.size);
        assert!(a.size > 0);
    }

    #[test]
    fn test_scan_sort_order() {
        let tmp = TempDir::new().unwrap();
        // Use sizes well above any filesystem block size to ensure different allocations
        fs::write(tmp.path().join("large.txt"), "a".repeat(100_000)).unwrap();
        fs::write(tmp.path().join("small.txt"), "b".repeat(10_000)).unwrap();
        fs::write(tmp.path().join("medium.txt"), "c".repeat(50_000)).unwrap();

        let result = scan_directory(tmp.path());
        let names: Vec<&str> = result.root.children.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0], "large.txt");
        assert_eq!(names[1], "medium.txt");
        assert_eq!(names[2], "small.txt");
    }

    #[test]
    fn test_scan_hardlinks_counted_once() {
        let tmp = TempDir::new().unwrap();
        let original = tmp.path().join("original.txt");
        fs::write(&original, "x".repeat(8192)).unwrap();
        let hardlink = tmp.path().join("hardlink.txt");
        fs::hard_link(&original, &hardlink).unwrap();

        let result = scan_directory(tmp.path());
        // Total size should equal ONE file's allocation, not two
        let total = result.root.size;
        let single_size = result
            .root
            .children
            .iter()
            .find(|e| e.name == "original.txt")
            .map(|e| e.size)
            .unwrap();
        assert_eq!(total, single_size, "hardlink must not be double-counted");
    }
}
