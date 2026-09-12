use std::fmt::Write;
use std::path::{Path, PathBuf};

use itertools::Itertools;
use smol_str::SmolStr;
use spago::lockfile::Lockfile;

const SPAGO_LOCK: &str = include_str!("./fixture/spago.lock");

fn source_snapshot(lockfile: &Lockfile) -> String {
    lockfile.sources().sorted().filter_map(normalize_source).join("\n")
}

fn source_by_package_snapshot(lockfile: &Lockfile) -> String {
    let mut snapshot = String::default();
    for (name, package) in lockfile.sources_by_package() {
        writeln!(&mut snapshot, "{name}: {:?}", package.reference).unwrap();
        for source in package.sources.into_iter().filter_map(normalize_source) {
            writeln!(&mut snapshot, "  {source}").unwrap();
        }
    }

    snapshot
}

fn normalize_source(source: PathBuf) -> Option<String> {
    let source = source.to_str()?;
    Some(source.replace('\\', "/"))
}

fn normalize_path(path: &mut PathBuf) {
    let normalized = path.to_string_lossy().replace('\\', "/");
    *path = PathBuf::from(normalized);
}

#[test]
fn test_parse_lockfile() {
    let lockfile = serde_json::from_str::<Lockfile>(SPAGO_LOCK);
    assert!(lockfile.is_ok(), "{lockfile:?}");
}

#[test]
fn test_lockfile_sources_subdir_precedence_packages_over_workspace_extra_packages() {
    let lockfile = serde_json::from_str::<Lockfile>(
        r#"{
  "workspace": {
    "packages": {},
    "extra_packages": {
      "foo": { "subdir": "from-workspace" }
    }
  },
  "packages": {
    "foo": { "type": "git", "rev": "abcd", "subdir": "from-packages" }
  }
}"#,
    )
    .unwrap();

    insta::assert_snapshot!(source_snapshot(&lockfile), @"
    .spago/p/foo/abcd/from-packages/src
    .spago/p/foo/abcd/from-packages/test
    .spago/p/foo/abcd/src
    .spago/p/foo/abcd/test
    ");
}

#[test]
fn test_lockfile_sources_subdir_fallback_to_workspace_extra_packages() {
    let lockfile = serde_json::from_str::<Lockfile>(
        r#"{
  "workspace": {
    "packages": {},
    "extra_packages": {
      "deku-core": { "subdir": "deku-core" }
    }
  },
  "packages": {
    "deku-core": { "type": "git", "rev": "65d6e9d" }
  }
}"#,
    )
    .unwrap();

    insta::assert_snapshot!(source_snapshot(&lockfile), @"
    .spago/p/deku-core/65d6e9d/deku-core/src
    .spago/p/deku-core/65d6e9d/deku-core/test
    .spago/p/deku-core/65d6e9d/src
    .spago/p/deku-core/65d6e9d/test
    ");
}

#[test]
fn test_parse_lockfile_without_extra_packages() {
    let lockfile = serde_json::from_str::<Lockfile>(
        r#"{
  "workspace": { "packages": {} },
  "packages": { "foo": { "type": "git", "rev": "abcd" } }
}"#,
    )
    .unwrap();

    insta::assert_snapshot!(source_snapshot(&lockfile), @"
    .spago/p/foo/abcd/src
    .spago/p/foo/abcd/test
    ");
}

#[test]
fn test_lockfile_sources_include_direct_dependencies() {
    let lockfile = serde_json::from_str::<Lockfile>(
        r#"{
  "workspace": {
    "packages": {
      "application": {
        "path": ".",
        "core": { "dependencies": ["effect", { "prelude": ">=6.0.0" }] },
        "test": { "dependencies": ["assert"] }
      }
    }
  },
  "packages": {
    "effect": {
      "type": "registry",
      "version": "4.0.0",
      "dependencies": ["prelude"]
    }
  }
}"#,
    )
    .unwrap();

    let packages = lockfile.sources_by_package();

    assert_eq!(
        packages["application"].dependencies.iter().map(SmolStr::as_str).collect_vec(),
        vec!["assert", "effect", "prelude"]
    );
    assert_eq!(
        packages["effect"].dependencies.iter().map(SmolStr::as_str).collect_vec(),
        vec!["prelude"]
    );
}

#[test]
fn test_lockfile_sources_by_package_include_package_roots() {
    let lockfile = serde_json::from_str::<Lockfile>(
        r#"{
  "workspace": {
    "packages": { "workspace-package": { "path": "packages/workspace-package" } },
    "extra_packages": { "git-package": { "subdir": "packages/git-package" } }
  },
  "packages": {
    "git-package": { "type": "git", "rev": "abcd" },
    "local-package": { "type": "local", "path": "../local-package" },
    "registry-package": { "type": "registry", "version": "1.2.3" }
  }
}"#,
    )
    .unwrap();

    let mut packages = lockfile.sources_by_package();
    for package in packages.values_mut() {
        for root in &mut package.roots {
            normalize_path(root);
        }
        for source in &mut package.sources {
            normalize_path(source);
        }
    }
    insta::assert_debug_snapshot!(packages, @r#"
    {
        "git-package": PackageSources {
            reference: Git {
                url: None,
                rev: "abcd",
                version: "abcd",
                subdir: Some(
                    "packages/git-package",
                ),
            },
            roots: [
                ".spago/p/git-package/abcd",
                ".spago/p/git-package/abcd/packages/git-package",
            ],
            sources: [
                ".spago/p/git-package/abcd/src",
                ".spago/p/git-package/abcd/test",
                ".spago/p/git-package/abcd/packages/git-package/src",
                ".spago/p/git-package/abcd/packages/git-package/test",
            ],
            dependencies: {},
        },
        "local-package": PackageSources {
            reference: Local,
            roots: [
                "../local-package",
            ],
            sources: [
                "../local-package/src",
                "../local-package/test",
            ],
            dependencies: {},
        },
        "registry-package": PackageSources {
            reference: Registry {
                version: "1.2.3",
            },
            roots: [
                ".spago/p/registry-package-1.2.3",
            ],
            sources: [
                ".spago/p/registry-package-1.2.3/src",
                ".spago/p/registry-package-1.2.3/test",
            ],
            dependencies: {},
        },
        "workspace-package": PackageSources {
            reference: Workspace,
            roots: [
                "packages/workspace-package",
            ],
            sources: [
                "packages/workspace-package/src",
                "packages/workspace-package/test",
            ],
            dependencies: {},
        },
    }
    "#);
}

#[test]
fn test_lockfile_sources() {
    let lockfile = serde_json::from_str::<Lockfile>(SPAGO_LOCK);
    assert!(lockfile.is_ok(), "{lockfile:?}");
    insta::assert_snapshot!(source_snapshot(&lockfile.unwrap()));
}

#[test]
fn test_lockfile_sources_by_package() {
    let lockfile = serde_json::from_str::<Lockfile>(SPAGO_LOCK);
    assert!(lockfile.is_ok(), "{lockfile:?}");
    insta::assert_snapshot!(source_by_package_snapshot(&lockfile.unwrap()));
}

#[test]
fn test_source_files_by_package() {
    let manifest_directory = env!("CARGO_MANIFEST_DIR");
    let manifest_directory_url = url::Url::from_file_path(manifest_directory).unwrap();
    let manifest_directory_uri = manifest_directory_url.to_string();

    let path = Path::new(manifest_directory);
    let fixture = path.join("tests/fixture");

    let mut snapshot = String::default();

    for (name, package) in spago::source_files_by_package(fixture).unwrap() {
        writeln!(&mut snapshot, "{name}: {:?}", package.reference).unwrap();
        for file in package.sources {
            let url = url::Url::from_file_path(file).unwrap();
            let uri = url.to_string().replace(&manifest_directory_uri, "./spago");
            writeln!(&mut snapshot, "  {uri}").unwrap();
        }
    }

    insta::assert_snapshot!(snapshot);
}
