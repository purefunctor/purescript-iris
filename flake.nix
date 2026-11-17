{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forEachSystem =
        function:
        nixpkgs.lib.genAttrs systems (
          system:
          function (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      devShells = forEachSystem (
        pkgs:
        let
          rust = pkgs.rust-bin.stable."1.100.0".minimal.override {
            extensions = [
              "clippy"
              "rustfmt"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };
          nightlyRust = pkgs.rust-bin.nightly."2026-11-16".minimal.override {
            extensions = [ "rustfmt" ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              pkgs.nodejs_22
              pkgs.pnpm_12
              pkgs.just
              pkgs.cargo-insta
              pkgs.cargo-nextest
              pkgs.cargo-edit
              pkgs.cargo-bundle-licenses
              pkgs.pkg-config
              pkgs.openssl
            ];
            IRIS_NIGHTLY_RUSTFMT = "${nightlyRust}/bin/rustfmt";
            shellHook = ''
              repository_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
              export PNPM_CONFIG_STORE_DIR="$repository_root/target/pnpm-store"
              export PNPM_CONFIG_CACHE_DIR="$repository_root/target/pnpm-cache"
              unset repository_root
            '';
          };

          verification = pkgs.mkShellNoCC {
            packages = [ pkgs.temurin-bin-25 ];
          };
        }
      );
    };
}
