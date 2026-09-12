use std::env;

fn main() {
    println!("cargo::rerun-if-env-changed=IRIS_BUILD_REVISION");

    let package_version = env::var("CARGO_PKG_VERSION")
        .expect("invariant violated: Cargo must provide package version");
    let version = match env::var("IRIS_BUILD_REVISION") {
        Ok(revision) => {
            assert!(
                (7..=64).contains(&revision.len())
                    && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "IRIS_BUILD_REVISION must contain 7 to 64 hexadecimal characters"
            );
            format!("{package_version}-dev.{}", revision.to_ascii_lowercase())
        }
        Err(env::VarError::NotPresent) => package_version,
        Err(env::VarError::NotUnicode(_)) => panic!("IRIS_BUILD_REVISION must be Unicode"),
    };

    println!("cargo::rustc-env=IRIS_VERSION={version}");
}
