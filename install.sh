#!/bin/sh

set -eu

repository="purefunctor/purescript-iris"
binary="iris"
install_directory="${IRIS_INSTALL_DIR:-$HOME/.local/bin}"
version="${IRIS_VERSION:-latest}"

fail() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required to install $binary"
command -v tar >/dev/null 2>&1 || fail "tar is required to install $binary"

case "$(uname -s):$(uname -m)" in
    Linux:x86_64 | Linux:amd64)
        target="x86_64-unknown-linux-musl"
        ;;
    Darwin:x86_64 | Darwin:arm64)
        target="universal-apple-darwin"
        ;;
    *)
        fail "unsupported platform: $(uname -s) $(uname -m)"
        ;;
esac

if [ "$version" = "latest" ]; then
    release_url=$(curl --proto '=https' --tlsv1.2 -LsSf -o /dev/null \
        -w '%{url_effective}' "https://github.com/$repository/releases/latest")
    version=${release_url##*/}
fi

case "$version" in
    v0.0.*) fail "this installer requires v0.1.0 or later; use the installer from the requested release tag for v0.0.x" ;;
    v[0-9]*) ;;
    *) fail "invalid release version: $version" ;;
esac

archive_name="$binary-$target.tar.gz"
archive_url="https://github.com/$repository/releases/download/$version/$archive_name"
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/iris-install.XXXXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT HUP INT TERM
archive="$temporary_directory/$archive_name"

printf 'Downloading %s %s for %s\n' "$binary" "$version" "$target"
curl --proto '=https' --tlsv1.2 -LsSf --retry 3 --output "$archive" "$archive_url"

if [ "${IRIS_SKIP_ATTESTATION:-}" = 1 ]; then
    printf '%s\n' 'warning: GitHub release attestation verification skipped (IRIS_SKIP_ATTESTATION=1).' >&2
elif command -v gh >/dev/null 2>&1 && gh attestation verify --help >/dev/null 2>&1; then
    printf 'Verifying GitHub release attestation\n'
    gh attestation verify "$archive" --repo "$repository" >/dev/null || \
        fail "GitHub release attestation verification failed"
else
    printf '%s\n' \
        'warning: A GitHub CLI with attestation support is not installed; release provenance was not verified.' \
        'warning: Install or update gh from https://cli.github.com/ to verify future installations.' >&2
fi

archive_directory="$binary-$target"
archive_binary="$archive_directory/$binary"
entry_count=$(tar -tzf "$archive" | grep -c "^$archive_binary$" || true)
[ "$entry_count" -eq 1 ] || fail "release archive does not contain exactly one $binary executable"

tar -xzf "$archive" -C "$temporary_directory" "$archive_binary"
extracted_binary="$temporary_directory/$archive_binary"
[ -f "$extracted_binary" ] && [ ! -L "$extracted_binary" ] || \
    fail "release archive contains an invalid $binary executable"

mkdir -p "$install_directory"
destination="$install_directory/$binary"
[ ! -L "$destination" ] || fail "refusing to replace symbolic link: $destination"
installation_file=$(mktemp "$install_directory/.iris-install.XXXXXXXX")
trap 'rm -rf "$temporary_directory"; rm -f "$installation_file"' EXIT HUP INT TERM
cp "$extracted_binary" "$installation_file"
chmod 755 "$installation_file"
mv -f "$installation_file" "$destination"

printf 'Installed %s to %s\n' "$version" "$destination"
case ":$PATH:" in
    *":$install_directory:"*) ;;
    *) printf 'Add %s to PATH to run %s.\n' "$install_directory" "$binary" ;;
esac
