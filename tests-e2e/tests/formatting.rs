use std::io::Write;

#[path = "support.rs"]
mod support;

use support::{OPERATING_SYSTEM_ERROR, TestWorkspace};

const UNFORMATTED: &str = "module Main where

value=1
";
const FORMATTED: &str = "module Main where

value = 1
";

#[test]
fn formats_standard_input_to_standard_output() {
    let workspace = TestWorkspace::empty();
    let unformatted = r#"module Main where

{- A block comment.
   With another line. -}
value="""first
second"""
"#
    .replace('\n', "\r\n");
    for arguments in [&["format", "--file", "-"][..], &["format", "--file", "-", "--file", "-"][..]]
    {
        let mut child = workspace.spawn(arguments);
        child.stdin.take().unwrap().write_all(unformatted.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        insta::allow_duplicates! {
            insta::assert_snapshot!(workspace.report(&output), @r#"
            status: 0
            --- stdout
            module Main where

            {- A block comment.
               With another line. -}
            value = """first\r
            second"""
            --- stderr
            "#);
        }
    }
}

#[test]
fn previews_a_file_without_writing_and_rejects_ambiguous_modes() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Main.purs", UNFORMATTED);
    workspace.write(
        "src/Other.purs",
        "module Other where
value=2
",
    );
    let preview = workspace.command(&["format", "--file", "src/Main.purs"]);
    insta::assert_snapshot!(workspace.report(&preview), @"
    status: 0
    --- stdout
    module Main where

    value = 1
    --- stderr
    ");
    let rejections = workspace.transcript(&[
        &["format", "--write", "--check"],
        &["format", "--write", "--file", "-"],
        &["format", "src/Main.purs"],
        &["format", "--file", "src/Main.purs", "--file", "src/Other.purs"],
        &["format", "--width", "0"],
        &["format", "--indent", "0"],
        &["format", "--indent", "65536"],
    ]);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("rejects_ambiguous_modes", rejections);
    });
    insta::assert_snapshot!(workspace.summary(), @"
    --- src/Main.purs
    module Main where

    value=1
    --- src/Other.purs
    module Other where
    value=2
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}

#[test]
fn wires_unicode_to_preview_write_and_check() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "src/Main.purs",
        r#"module Main where

identity :: forall a. a -> a
identity = \value -> value
"#,
    );
    let commands = workspace.transcript(&[
        &["format", "--file", "src/Main.purs"],
        &["format", "--check", "--file", "src/Main.purs"],
        &["format", "--unicode", "--file", "src/Main.purs"],
        &["format", "--unicode", "--check", "--file", "src/Main.purs"],
        &["format", "--unicode", "--write", "--file", "src/Main.purs"],
        &["format", "--unicode", "--check", "--file", "src/Main.purs"],
        &["format", "--check", "--file", "src/Main.purs"],
    ]);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("unicode", commands);
    });
    insta::assert_snapshot!(workspace.summary(), @r"
    --- src/Main.purs
    module Main where

    identity ∷ ∀ a. a → a
    identity = \value → value
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}

#[test]
fn wires_width_and_indent_to_preview_stdin_write_and_check() {
    let workspace = TestWorkspace::empty();
    let source = "module Main where

value=combine firstArgument secondArgument
";
    workspace.write("src/Main.purs", source);
    let mut child = workspace.spawn(&["format", "--width", "32", "--indent", "4", "--file", "-"]);
    child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
    let standard_input = child.wait_with_output().unwrap();
    insta::assert_snapshot!(workspace.report(&standard_input), @"
    status: 0
    --- stdout
    module Main where

    value = combine firstArgument
        secondArgument
    --- stderr
    ");
    let commands = workspace.transcript(&[
        &["format", "--file", "src/Main.purs"],
        &["format", "--width", "32", "--file", "src/Main.purs"],
        &["format", "--width", "32", "--indent", "4", "--file", "src/Main.purs"],
        &["format", "--width", "32", "--indent", "4", "--write", "--file", "src/Main.purs"],
        &["format", "--width", "32", "--indent", "4", "--check", "--file", "src/Main.purs"],
        &["format", "--check", "--file", "src/Main.purs"],
    ]);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("width_and_indent", commands);
    });
    insta::assert_snapshot!(workspace.summary(), @"
    --- src/Main.purs
    module Main where

    value = combine firstArgument
        secondArgument
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}

#[test]
fn writes_checks_and_is_idempotent() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Main.purs", UNFORMATTED);
    let commands = workspace.transcript(&[
        &["format", "--check", "--file", "src/Main.purs"],
        &["format", "--write", "--file", "src/Main.purs"],
        &["format", "--check", "--file", "src/Main.purs"],
        &["format", "--write", "--file", "src/Main.purs"],
    ]);
    insta::assert_snapshot!(commands, @"
    $ iris format --check --file src/Main.purs
    status: 1
    --- stdout
    --- stderr
    src/Main.purs
    $ iris format --write --file src/Main.purs
    status: 0
    --- stdout
    --- stderr
    $ iris format --check --file src/Main.purs
    status: 0
    --- stdout
    --- stderr
    $ iris format --write --file src/Main.purs
    status: 0
    --- stdout
    --- stderr
    ");
    insta::assert_snapshot!(workspace.summary(), @"
    --- src/Main.purs
    module Main where

    value = 1
    ");
}

#[test]
fn malformed_batch_does_not_modify_valid_files() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "spago.yaml",
        "package:
  name: application
  dependencies: []
workspace: {}
",
    );
    workspace.write("src/AValid.purs", UNFORMATTED);
    workspace.write("src/ZInvalid.purs", "module");
    let commands = workspace.transcript(&[
        &["format"],
        &["format", "--write", "--file", "src/AValid.purs", "--file", "src/ZInvalid.purs"],
    ]);
    insta::assert_snapshot!(commands, @"
    $ iris format
    status: 2
    --- stdout
    --- stderr
    error: [WORKSPACE]/src/ZInvalid.purs: cannot format: 1:7: Expected UPPER
    $ iris format --write --file src/AValid.purs --file src/ZInvalid.purs
    status: 2
    --- stdout
    --- stderr
    error: src/ZInvalid.purs: cannot format: 1:7: Expected UPPER
    ");
    insta::assert_snapshot!(workspace.summary(), @"
    --- spago.yaml
    package:
      name: application
      dependencies: []
    workspace: {}
    --- src/AValid.purs
    module Main where

    value=1
    --- src/ZInvalid.purs
    module
    [no final newline]
    ");
}

#[test]
fn formats_the_entire_workspace_from_a_nested_package_without_fetching_dependencies() {
    let workspace = TestWorkspace::empty();
    workspace.write("spago.yaml", "workspace: {}\n");
    for package in ["first", "second"] {
        workspace.write(
            &format!("packages/{package}/spago.yaml"),
            &format!(
                "package:
  name: {package}
  dependencies: [unfetched]
"
            ),
        );
    }
    let sources = [
        "packages/first/src/Main.purs",
        "packages/first/src/Inner/Foo.purs",
        "packages/first/src/Inner/src/Inside.purs",
        "packages/first/src/ManifestOnly/Leaf.purs",
        "packages/first/test/nested/Test.purs",
        "packages/second/src/Main.purs",
        "packages/second/test/Test.purs",
    ];
    for path in sources {
        workspace.write(path, UNFORMATTED);
    }
    workspace.write(
        "packages/first/src/Inner/spago.yaml",
        "package:
  name: inner-member
  dependencies: []
",
    );
    workspace.write("packages/first/src/ManifestOnly/spago.yaml", "{}\n");
    workspace.write(
        "nested/spago.yaml",
        "package:
  name: nested
workspace: {}
",
    );
    workspace.write(
        "packages/first/src/nested-workspace/spago.yaml",
        "package:
  name: inner
workspace: {}
",
    );
    workspace.write(
        ".spago/p/dependency/spago.yaml",
        "package:
  name: dependency
  dependencies: []
",
    );
    workspace.write(
        "node_modules/dependency/spago.yaml",
        "package:
  name: node-dependency
  dependencies: []
",
    );
    let excluded = [
        "src/Unowned.purs",
        ".spago/p/dependency/src/Dependency.purs",
        "node_modules/dependency/src/Dependency.purs",
        "output/Generated.purs",
        "nested/src/Main.purs",
        "packages/first/src/nested-workspace/src/Main.purs",
    ];
    for path in excluded {
        workspace.write(path, UNFORMATTED);
    }
    let unformatted = workspace.summary();
    let check = workspace.command_in("packages/first/src", &["format", "--check"]);
    assert_eq!(workspace.summary(), unformatted);
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("check_workspace", workspace.report(&check));
    });
    let write = workspace.command_in("packages/first/src", &["format"]);
    insta::assert_snapshot!(workspace.report(&write), @"
    status: 0
    --- stdout
    --- stderr
    ");
    let check = workspace.command(&["format", "--check"]);
    insta::assert_snapshot!(workspace.report(&check), @"
    status: 0
    --- stdout
    --- stderr
    ");
    insta::with_settings!({omit_expression => true}, {
        insta::assert_snapshot!("formatted_workspace", workspace.summary());
    });
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}

#[test]
fn workspace_source_discovery_stays_within_literal_package_directories() {
    let workspace = TestWorkspace::empty();
    workspace.write(
        "project[12]/spago.yaml",
        "package:
  name: application
  dependencies: []
workspace: {}
",
    );
    workspace.write("project[12]/src/Main.purs", UNFORMATTED);
    workspace.write("project[12]/src/.purs", UNFORMATTED);
    workspace.write("project[12]/src/Directory.purs/Nested.purs", UNFORMATTED);
    workspace.write("project1/src/Main.purs", UNFORMATTED);
    workspace.write("project[12]/test/spago.yaml", "workspace: {}\n");
    workspace.write("project[12]/test/src/Isolated.purs", UNFORMATTED);
    let output = workspace.command_in("project[12]", &["format"]);
    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    --- stderr
    ");
    insta::assert_snapshot!(workspace.summary(), @"
    --- project1/src/Main.purs
    module Main where

    value=1
    --- project[12]/spago.yaml
    package:
      name: application
      dependencies: []
    workspace: {}
    --- project[12]/src/.purs
    module Main where

    value = 1
    --- project[12]/src/Directory.purs/Nested.purs
    module Main where

    value = 1
    --- project[12]/src/Main.purs
    module Main where

    value = 1
    --- project[12]/test/spago.yaml
    workspace: {}
    --- project[12]/test/src/Isolated.purs
    module Main where

    value=1
    ");
}

#[test]
fn file_mode_is_literal_repeatable_and_independent_of_the_workspace() {
    let workspace = TestWorkspace::empty();
    workspace.write("spago.yaml", "invalid: [manifest");
    workspace.write("src/Main.purs", UNFORMATTED);
    workspace.write("src/[Literal].purs", UNFORMATTED);
    workspace.write("src/Unrequested.purs", "module");
    let commands = workspace.transcript(&[
        &["format", "--write", "--file", "src"],
        &["format", "--write", "--file", "src/*.purs"],
        &[
            "format",
            "--write",
            "--file",
            "src/Main.purs",
            "--file",
            "src/[Literal].purs",
            "--file",
            "src/Main.purs",
        ],
    ]);
    insta::with_settings!({filters => vec![OPERATING_SYSTEM_ERROR]}, {
        insta::assert_snapshot!(commands, @"
        $ iris format --write --file src
        status: 2
        --- stdout
        --- stderr
        error: could not read src: [OS ERROR]
        $ iris format --write --file src/*.purs
        status: 2
        --- stdout
        --- stderr
        error: could not read src/*.purs: [OS ERROR]
        $ iris format --write --file src/Main.purs --file src/[Literal].purs --file src/Main.purs
        status: 0
        --- stdout
        --- stderr
        ");
    });
    insta::assert_snapshot!(workspace.summary(), @"
    --- spago.yaml
    invalid: [manifest
    [no final newline]
    --- src/Main.purs
    module Main where

    value = 1
    --- src/Unrequested.purs
    module
    [no final newline]
    --- src/[Literal].purs
    module Main where

    value = 1
    ");
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}

#[test]
fn workspace_mode_requires_a_spago_workspace_instead_of_reading_stdin() {
    let workspace = TestWorkspace::empty();
    workspace.write("src/Main.purs", UNFORMATTED);
    let output = workspace.command(&["format"]);
    insta::assert_snapshot!(workspace.report(&output), @"
    status: 2
    --- stdout
    --- stderr
    error: no Spago workspace found from [WORKSPACE]
    ");
    insta::assert_snapshot!(workspace.summary(), @"
    --- src/Main.purs
    module Main where

    value=1
    ");
}

#[cfg(unix)]
#[test]
fn skips_discovered_symlinks_and_refuses_to_replace_explicit_file_links() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let workspace = TestWorkspace::empty();
    workspace.write("src/Main.purs", UNFORMATTED);
    workspace.write("targets/Source.purs", UNFORMATTED);
    symlink("../targets/Source.purs", workspace.path().join("src/ZLinked.purs")).unwrap();

    let preview = workspace.command(&["format", "--file", "src/ZLinked.purs"]);
    insta::assert_snapshot!(workspace.report(&preview), @"
    status: 0
    --- stdout
    module Main where

    value = 1
    --- stderr
    ");
    let write = workspace.command(&[
        "format",
        "--write",
        "--file",
        "src/Main.purs",
        "--file",
        "src/ZLinked.purs",
    ]);
    insta::assert_snapshot!(workspace.report(&write), @"
    status: 2
    --- stdout
    --- stderr
    error: refusing to replace symlink src/ZLinked.purs
    ");
    insta::assert_snapshot!(workspace.summary(), @"
    --- src/Main.purs
    module Main where

    value=1
    --- src/ZLinked.purs
    module Main where

    value=1
    --- targets/Source.purs
    module Main where

    value=1
    ");
    assert!(fs::symlink_metadata(workspace.path().join("src/ZLinked.purs")).unwrap().is_symlink());

    workspace.write(
        "spago.yaml",
        "package:
  name: application
  dependencies: []
workspace: {}
",
    );
    symlink("../targets", workspace.path().join("src/linked-directory")).unwrap();
    symlink("targets", workspace.path().join("test")).unwrap();
    let output = workspace.command(&["format"]);
    insta::assert_snapshot!(workspace.report(&output), @"
    status: 0
    --- stdout
    --- stderr
    ");
    insta::assert_snapshot!(workspace.summary(), @"
    --- spago.yaml
    package:
      name: application
      dependencies: []
    workspace: {}
    --- src/Main.purs
    module Main where

    value = 1
    --- src/ZLinked.purs
    module Main where

    value=1
    --- src/linked-directory/Source.purs
    module Main where

    value=1
    --- targets/Source.purs
    module Main where

    value=1
    --- test/Source.purs
    module Main where

    value=1
    ");
    assert!(fs::symlink_metadata(workspace.path().join("src/ZLinked.purs")).unwrap().is_symlink());
    insta::assert_snapshot!(workspace.spago_calls(), @"");
}

#[cfg(unix)]
#[test]
fn writes_preserve_file_identity_permissions_and_hard_links() {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let workspace = TestWorkspace::empty();
    workspace.write("Main.purs", UNFORMATTED);
    let path = workspace.path().join("Main.purs");
    fs::hard_link(&path, workspace.path().join("Linked.purs")).unwrap();
    for mode in [0o640, 0o750] {
        workspace.write("Main.purs", UNFORMATTED);
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        let original = fs::metadata(&path).unwrap();
        let output = workspace.command(&["format", "--write", "--file", "Main.purs"]);
        insta::allow_duplicates! {
            insta::assert_snapshot!(workspace.report(&output), @"
            status: 0
            --- stdout
            --- stderr
            ");
            insta::assert_snapshot!(workspace.summary(), @"
            --- Linked.purs
            module Main where

            value = 1
            --- Main.purs
            module Main where

            value = 1
            ");
        }
        let formatted = fs::metadata(&path).unwrap();
        assert_eq!(formatted.permissions().mode() & 0o7777, mode);
        assert_eq!(formatted.uid(), original.uid());
        assert_eq!(formatted.gid(), original.gid());
        assert_eq!(formatted.dev(), original.dev());
        assert_eq!(formatted.ino(), original.ino());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn writes_preserve_access_acls_without_inheriting_new_entries() {
    let workspace = TestWorkspace::empty();
    workspace.write("Main.purs", UNFORMATTED);
    let path = workspace.path().join("Main.purs");
    let mut acl = 2_u32.to_le_bytes().to_vec();
    for (tag, permissions, id) in [
        (1_u16, 6_u16, u32::MAX),
        (2, 4, 12345),
        (4, 0, u32::MAX),
        (16, 4, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        acl.extend(tag.to_le_bytes());
        acl.extend(permissions.to_le_bytes());
        acl.extend(id.to_le_bytes());
    }
    xattr::set(workspace.path(), "system.posix_acl_default", &acl).unwrap();
    for access_acl in [Some(&acl), None] {
        workspace.write("Main.purs", UNFORMATTED);
        match access_acl {
            Some(acl) => xattr::set(&path, "system.posix_acl_access", acl).unwrap(),
            None => xattr::remove(&path, "system.posix_acl_access").unwrap(),
        }
        let original = xattr::get(&path, "system.posix_acl_access").unwrap();
        let output = workspace.command(&["format", "--write", "--file", "Main.purs"]);
        support::assert_success(&output);
        assert_eq!(workspace.read("Main.purs"), FORMATTED);
        assert_eq!(xattr::get(&path, "system.posix_acl_access").unwrap(), original);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn writes_preserve_darwin_acls_without_inheriting_new_entries() {
    use std::process::Command;

    let workspace = TestWorkspace::empty();
    workspace.write("Main.purs", UNFORMATTED);
    let path = workspace.path().join("Main.purs");
    support::assert_success(
        &Command::new("/bin/chmod")
            .args(["+a", "everyone allow read,file_inherit"])
            .arg(workspace.path())
            .output()
            .unwrap(),
    );
    let access_rules = || {
        let output = Command::new("/bin/ls").arg("-le").arg(&path).output().unwrap();
        support::assert_success(&output);
        let output = String::from_utf8(output.stdout).unwrap();
        output.lines().skip(1).map(str::to_owned).collect::<Vec<_>>()
    };
    for has_acl in [true, false] {
        workspace.write("Main.purs", UNFORMATTED);
        support::assert_success(&Command::new("/bin/chmod").arg("-N").arg(&path).output().unwrap());
        if has_acl {
            support::assert_success(
                &Command::new("/bin/chmod")
                    .args(["+a", "everyone allow read"])
                    .arg(&path)
                    .output()
                    .unwrap(),
            );
        }
        let original = access_rules();
        assert_eq!(!original.is_empty(), has_acl);
        support::assert_success(&workspace.command(&["format", "--write", "--file", "Main.purs"]));
        assert_eq!(workspace.read("Main.purs"), FORMATTED);
        assert_eq!(access_rules(), original);
    }
}

#[cfg(windows)]
#[test]
fn writes_preserve_windows_security_descriptors_without_delete_access() {
    use std::fs;
    use std::process::Command;

    let workspace = TestWorkspace::empty();
    let path = workspace.path().join("Main.purs");
    let identity = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        ])
        .output()
        .unwrap();
    support::assert_success(&identity);
    let identity = String::from_utf8(identity.stdout).unwrap();
    // Modify excludes FILE_DELETE_CHILD, so the file's DELETE deny cannot be bypassed.
    support::assert_success(
        &Command::new("icacls.exe")
            .arg(workspace.path())
            .args(["/inheritance:r", "/grant:r", &format!("*{}:(OI)(CI)M", identity.trim())])
            .output()
            .unwrap(),
    );
    let security_descriptor = || {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$accessControls = [System.IO.File]::GetAccessControl($env:IRIS_ACL_PATH)
$accessControls.GetSecurityDescriptorSddlForm('All')",
            ])
            .env("IRIS_ACL_PATH", &path)
            .output()
            .unwrap();
        support::assert_success(&output);
        String::from_utf8(output.stdout).unwrap()
    };
    for protected in [false, true] {
        workspace.write("Main.purs", UNFORMATTED);
        support::assert_success(
            &Command::new("icacls.exe").arg(&path).args(["/grant", "*S-1-1-0:R"]).output().unwrap(),
        );
        let inheritance = if protected { "/inheritance:d" } else { "/inheritance:e" };
        support::assert_success(
            &Command::new("icacls.exe").arg(&path).arg(inheritance).output().unwrap(),
        );
        let original = security_descriptor();
        assert!(!original.trim().is_empty());
        support::assert_success(&workspace.command(&["format", "--write", "--file", "Main.purs"]));
        assert_eq!(workspace.read("Main.purs"), FORMATTED);
        assert_eq!(security_descriptor(), original);
    }

    workspace.write("Main.purs", UNFORMATTED);
    support::assert_success(
        &Command::new("icacls.exe").arg(&path).args(["/deny", "*S-1-1-0:(DE)"]).output().unwrap(),
    );
    struct RestoreDeleteAccess<'a>(&'a std::path::Path);

    impl Drop for RestoreDeleteAccess<'_> {
        fn drop(&mut self) {
            let _ = Command::new("icacls.exe").arg(self.0).args(["/remove:d", "*S-1-1-0"]).output();
        }
    }

    let _restore_delete_access = RestoreDeleteAccess(&path);
    let original_access_controls = security_descriptor();
    let output = workspace.command(&["format", "--write", "--file", "Main.purs"]);
    support::assert_success(&output);
    let source = workspace.read("Main.purs");
    let formatted_access_controls = security_descriptor();
    let entries = fs::read_dir(workspace.path()).unwrap().map(|entry| entry.unwrap().file_name());
    let entries = entries.collect::<Vec<_>>();
    assert_eq!(source, FORMATTED);
    assert_eq!(formatted_access_controls, original_access_controls);
    assert_eq!(entries, ["Main.purs"]);
}
