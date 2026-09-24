//! Source glob expansion for build preparation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use path_absolutize::Absolutize;
use thiserror::Error;
use walkdir::WalkDir;

pub struct Walk {
    pub roots: BTreeSet<PathBuf>,
    pub globs: GlobSet,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    GlobSetError(#[from] globset::Error),
    #[error(transparent)]
    WalkDirError(#[from] walkdir::Error),
}

pub fn walk_filtered(
    root: &Path,
    includes: impl IntoIterator<Item = impl AsRef<Path>>,
    excludes: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Result<Walk, Error> {
    let mut files = vec![];

    let mut roots: BTreeMap<PathBuf, GlobSetBuilder> = BTreeMap::default();
    let mut globs = GlobSetBuilder::new();

    for path in includes {
        let path = dunce::simplified(root).join(path);
        if let Ok(path) = path.absolutize()
            && let Some(path) = path.to_str()
            && let Ok(glob) = Glob::new(path)
        {
            roots
                .entry(glob_literal_base(path))
                .or_insert_with(GlobSetBuilder::new)
                .add(glob.clone());
            globs.add(glob);
        } else {
            files.push(path);
        }
    }

    let globs = globs.build()?;
    let excludes = build_excludes(root, excludes)?;
    files.retain(|path| !excludes.is_match(path));
    let mut files_from_glob = BTreeSet::default();

    for (root, root_globs) in &roots {
        if !root.exists() {
            continue;
        }

        let root_globs = root_globs.build()?;
        let entries =
            WalkDir::new(root).into_iter().filter_entry(|entry| !excludes.is_match(entry.path()));
        for entry in entries {
            let path = entry?.into_path();
            if root_globs.is_match(&path) {
                files_from_glob.insert(path);
            }
        }
    }

    files.extend(files_from_glob);

    let roots = roots.into_keys().collect();
    Ok(Walk { roots, globs, files })
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
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        touch(&root.join("package/src/Main.purs"));
        touch(&root.join("package/test/Test.Main.purs"));
        touch(&root.join("package/test/Excluded.purs"));

        let walk = walk_filtered(
            root,
            ["package/src/**/*.purs", "package/test/**/*.purs"],
            ["package/test/Excluded.purs"],
        )
        .unwrap();

        assert_eq!(
            relative_files(root, walk.files),
            vec!["package/src/Main.purs", "package/test/Test.Main.purs"]
        );
    }

    #[test]
    fn filtered_walk_excludes_directory_trees() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        touch(&root.join("package/src/Main.purs"));
        touch(&root.join("package/src/generated/Ignored.purs"));

        let walk =
            walk_filtered(root, ["package/src/**/*.purs"], ["package/src/generated"]).unwrap();

        assert_eq!(relative_files(root, walk.files), vec!["package/src/Main.purs"]);
    }

    #[test]
    fn filtered_walk_matches_globs_with_nested_literal_bases() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        touch(&root.join("package/src/Main.purs"));
        touch(&root.join("package/src/nested/Test.purs"));
        touch(&root.join("package/src/nested/Other.purs"));

        let walk = walk_filtered(
            root,
            ["package/**/Test.purs", "package/src/*Main.purs", "package/src/**/Test.purs"],
            std::iter::empty::<&Path>(),
        )
        .unwrap();

        assert_eq!(
            relative_files(root, walk.files),
            vec!["package/src/Main.purs", "package/src/nested/Test.purs"]
        );
        assert!(walk.globs.is_match(root.join("package/src/nested/Test.purs")));
        assert_eq!(walk.roots, BTreeSet::from([root.join("package"), root.join("package/src")]));
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
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        touch(&root.join("src/Main.purs"));
        let canonical_root = dunce::canonicalize(root).unwrap();

        let walk = walk_filtered(&canonical_root, [r"src\**\*.purs"], std::iter::empty::<&Path>())
            .unwrap();

        assert_eq!(relative_files(&canonical_root, walk.files), vec!["src/Main.purs"]);
    }
}
