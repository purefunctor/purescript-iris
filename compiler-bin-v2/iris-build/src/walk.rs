//! Source glob expansion for build preparation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use path_absolutize::Absolutize;
use thiserror::Error;
use walkdir::WalkDir;

pub struct Walk {
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    GlobSetError(#[from] globset::Error),
    #[error(transparent)]
    WalkDirError(#[from] walkdir::Error),
}

pub fn walk(root: &Path, paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Result<Walk, Error> {
    walk_filtered(root, paths, std::iter::empty::<&Path>())
}

pub fn walk_filtered(
    root: &Path,
    includes: impl IntoIterator<Item = impl AsRef<Path>>,
    excludes: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Result<Walk, Error> {
    let mut files = vec![];

    let mut roots = BTreeSet::default();
    let mut globs = GlobSetBuilder::new();

    for path in includes {
        let path = dunce::simplified(root).join(path);
        if let Ok(path) = path.absolutize()
            && let Some(path) = path.to_str()
            && let Ok(glob) = Glob::new(path)
        {
            roots.insert(glob_literal_base(path));
            globs.add(glob);
        } else {
            files.push(path);
        }
    }

    let globs = globs.build()?;
    let excludes = build_excludes(root, excludes)?;
    files.retain(|path| !excludes.is_match(path));
    let mut files_from_glob = BTreeSet::default();

    for root in &roots {
        if !root.exists() {
            continue;
        }

        for entry in WalkDir::new(root) {
            let path = entry?.into_path();
            if globs.is_match(&path) && !excludes.is_match(&path) {
                files_from_glob.insert(path);
            }
        }
    }

    files.extend(files_from_glob);

    Ok(Walk { files })
}

fn build_excludes(
    root: &Path,
    excludes: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Result<GlobSet, Error> {
    let mut globs = GlobSetBuilder::new();

    for path in excludes {
        let path = dunce::simplified(root).join(path);
        if let Ok(path) = path.absolutize()
            && let Some(path) = path.to_str()
        {
            globs.add(Glob::new(path)?);
        }
    }

    Ok(globs.build()?)
}

fn glob_literal_base(pattern: &str) -> PathBuf {
    let mut base = PathBuf::new();
    for component in Path::new(pattern).components() {
        if component.as_os_str().to_string_lossy().chars().any(glob_syntax_character) {
            break;
        }
        base.push(component);
    }
    base
}

fn glob_syntax_character(character: char) -> bool {
    matches!(character, '*' | '?' | '[' | '{')
        || (character == '\\' && !std::path::is_separator('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("iris-walk-{nanos}"));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    fn relative_files(root: &Path, files: Vec<PathBuf>) -> Vec<String> {
        let files = files
            .into_iter()
            .map(|file| file.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
        let mut files = files.collect_vec();
        files.sort();
        files
    }

    #[test]
    fn filtered_walk_excludes_matching_files() {
        let root = temporary_directory();
        touch(&root.join("package/src/Main.purs"));
        touch(&root.join("package/test/Test.Main.purs"));
        touch(&root.join("package/test/Excluded.purs"));

        let walk = walk_filtered(
            &root,
            ["package/src/**/*.purs", "package/test/**/*.purs"],
            ["package/test/Excluded.purs"],
        )
        .unwrap();

        assert_eq!(
            relative_files(&root, walk.files),
            vec!["package/src/Main.purs", "package/test/Test.Main.purs"]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn literal_base_stops_at_the_first_wildcard() {
        let base = glob_literal_base("/workspace/src/**/*.purs");
        assert_eq!(base, PathBuf::from("/workspace/src"));
    }

    #[test]
    fn literal_base_excludes_a_wildcard_in_the_final_component() {
        let base = glob_literal_base("/workspace/src/*.purs");
        assert_eq!(base, PathBuf::from("/workspace/src"));
    }

    #[test]
    fn literal_base_retains_parent_directories() {
        let base = glob_literal_base("/workspace/../shared/src/**/*.purs");
        assert_eq!(base, PathBuf::from("/workspace/../shared/src"));
    }

    #[test]
    fn literal_base_of_a_pattern_without_wildcards_is_the_whole_path() {
        let base = glob_literal_base("/workspace/src/Main.purs");
        assert_eq!(base, PathBuf::from("/workspace/src/Main.purs"));
    }

    #[test]
    fn literal_base_recognises_every_metacharacter() {
        for pattern in ["/a/b/*.purs", "/a/b/?.purs", "/a/b/[abc].purs", "/a/b/{x,y}.purs"] {
            assert_eq!(glob_literal_base(pattern), PathBuf::from("/a/b"), "pattern: {pattern}");
        }
    }

    #[test]
    fn literal_base_keeps_class_closer_as_a_literal() {
        let pattern = "/workspace/src/Main].purs";

        assert!(Glob::new(pattern).is_ok());
        assert_eq!(glob_literal_base(pattern), PathBuf::from(pattern));
    }

    #[cfg(unix)]
    #[test]
    fn literal_base_stops_at_backslash_escape() {
        let pattern = "/workspace/src/\\*.purs";

        assert!(Glob::new(pattern).is_ok());
        assert_eq!(glob_literal_base(pattern), PathBuf::from("/workspace/src"));
    }

    #[cfg(windows)]
    #[test]
    fn walks_windows_style_globs() {
        let root = temporary_directory();
        touch(&root.join("src/Main.purs"));
        let canonical_root = dunce::canonicalize(&root).unwrap();

        let walk = walk(&canonical_root, [r"src\**\*.purs"]).unwrap();

        assert_eq!(relative_files(&canonical_root, walk.files), vec!["src/Main.purs"]);
        fs::remove_dir_all(root).unwrap();
    }
}
