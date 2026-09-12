#!/bin/sh
# Installs notopod from the binaries attached to a GitHub release.
# See `install.sh --help`, or the README.

set -eu

REPO="Yuvraj-cyborg/notopod"
VERSION="${NOTOPOD_VERSION:-latest}"
INSTALL_DIR="${NOTOPOD_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
note() { printf '  %s\n' "$*"; }
err() {
	printf '\nnotopod: %s\n' "$1" >&2
	shift
	for line in "$@"; do printf '  %s\n' "$line" >&2; done
	exit 1
}
have() { command -v "$1" >/dev/null 2>&1; }

# Written out rather than read back from this file, because when the
# script arrives through a pipe there is no file to read.
usage() {
	cat <<'EOF'
Installs notopod: the right prebuilt binary for this machine, from the
latest GitHub release.

  curl -fsSL https://raw.githubusercontent.com/Yuvraj-cyborg/notopod/main/install.sh | sh

Options, as flags or as environment variables:

  --to DIR       NOTOPOD_INSTALL_DIR   where the binary goes (default: ~/.local/bin)
  --version TAG  NOTOPOD_VERSION       a release tag, e.g. v0.3.0 (default: the latest)
  --help

Through a pipe, flags go after `-s --`:

  curl -fsSL <url> | sh -s -- --to /usr/local/bin

Nothing is written outside the install directory, and the download is
checked against the release's own sha256 before anything is installed.
EOF
}

while [ $# -gt 0 ]; do
	case "$1" in
	--to)
		[ $# -ge 2 ] || err "--to needs a directory"
		INSTALL_DIR="$2"
		shift 2
		;;
	--to=*)
		INSTALL_DIR="${1#*=}"
		shift
		;;
	--version)
		[ $# -ge 2 ] || err "--version needs a tag"
		VERSION="$2"
		shift 2
		;;
	--version=*)
		VERSION="${1#*=}"
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*) err "unknown option: $1" "run with --help to see what there is" ;;
	esac
done

# 0.3.0 and v0.3.0 both mean the tag v0.3.0.
case "$VERSION" in
latest | v*) ;;
*) VERSION="v$VERSION" ;;
esac

# ----- which build -----

os=$(uname -s)
arch=$(uname -m)
build_it="build it instead: cargo install --git https://github.com/$REPO"

case "$os" in
Linux)
	# The Linux binaries are built against glibc.
	if have ldd && ldd --version 2>&1 | head -1 | grep -qi musl; then
		err "these binaries need glibc, and this looks like a musl system" "$build_it"
	fi
	case "$arch" in
	x86_64 | amd64) target="x86_64-unknown-linux-gnu" ;;
	aarch64 | arm64) target="aarch64-unknown-linux-gnu" ;;
	*) err "no prebuilt binary for Linux $arch" "$build_it" ;;
	esac
	;;
Darwin)
	case "$arch" in
	x86_64) target="x86_64-apple-darwin" ;;
	arm64 | aarch64) target="aarch64-apple-darwin" ;;
	*) err "no prebuilt binary for macOS $arch" "$build_it" ;;
	esac
	;;
MINGW* | MSYS* | CYGWIN* | Windows_NT)
	err "this script does not do Windows" \
		"take the .zip from https://github.com/$REPO/releases/latest" \
		"or run: cargo install --git https://github.com/$REPO"
	;;
*) err "unsupported system: $os" "$build_it" ;;
esac

archive="notopod-$target.tar.gz"
checksum="notopod-$target.sha256"
if [ "$VERSION" = latest ]; then
	base="https://github.com/$REPO/releases/latest/download"
else
	base="https://github.com/$REPO/releases/download/$VERSION"
fi

# ----- fetch -----

# Failures are reported by the caller, in words; the tool's own noise
# about them is not useful here.
if have curl; then
	fetch() { curl -fsSL --retry 3 -o "$2" "$1" 2>/dev/null; }
elif have wget; then
	fetch() { wget -qO "$2" "$1" 2>/dev/null; }
else
	err "no curl and no wget, so there is nothing to download with"
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/notopod.XXXXXX")
# shellcheck disable=SC2064 # $tmp is fixed here on purpose
trap "rm -rf '$tmp'" EXIT INT TERM

say ""
say "notopod  →  $INSTALL_DIR"
note "$target, $([ "$VERSION" = latest ] && echo "latest release" || echo "$VERSION")"

fetch "$base/$archive" "$tmp/$archive" ||
	err "could not download $base/$archive" \
		"check the tag exists: https://github.com/$REPO/releases"

# ----- check it is what the release says it is -----

if fetch "$base/$checksum" "$tmp/$checksum" 2>/dev/null; then
	if have sha256sum; then
		check() { (cd "$tmp" && sha256sum -c "$checksum" >/dev/null 2>&1); }
	elif have shasum; then
		check() { (cd "$tmp" && shasum -a 256 -c "$checksum" >/dev/null 2>&1); }
	else
		check() { return 2; }
	fi
	set +e
	check
	checked=$?
	set -e
	case "$checked" in
	0) note "checksum ok" ;;
	2) note "no sha256 tool here, so the checksum was not verified" ;;
	*) err "the download does not match its published checksum" \
		"something went wrong upstream or in between; nothing was installed" ;;
	esac
else
	note "no checksum published for this build"
fi

# ----- install -----

tar -xzf "$tmp/$archive" -C "$tmp" || err "could not unpack $archive"
binary=$(find "$tmp" -type f -name notopod | head -1)
[ -n "$binary" ] || err "no notopod binary inside $archive"

mkdir -p "$INSTALL_DIR" 2>/dev/null || err "cannot create $INSTALL_DIR" \
	"pick somewhere you can write: --to ~/bin"
[ -w "$INSTALL_DIR" ] || err "cannot write to $INSTALL_DIR" \
	"pick somewhere you can write: --to ~/.local/bin" \
	"or run this script under sudo to keep $INSTALL_DIR"

# Write beside the target and rename over it, so a running notopod is
# never a half-written file.
staged="$INSTALL_DIR/.notopod.$$"
cp "$binary" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$INSTALL_DIR/notopod"

version=$("$INSTALL_DIR/notopod" --version 2>/dev/null || echo notopod)
note "installed $version"

# ----- is it on PATH? -----

case ":$PATH:" in
*":$INSTALL_DIR:"*)
	say ""
	say "Try it:  notopod ~/notes"
	say ""
	;;
*)
	# shellcheck disable=SC2088 # these are printed for a human to read
	case "${SHELL:-}" in
	*/zsh) rc="~/.zshrc" ;;
	*/bash) rc="~/.bashrc" ;;
	*/fish) rc="~/.config/fish/config.fish" ;;
	*) rc="your shell's startup file" ;;
	esac
	say ""
	say "$INSTALL_DIR is not on your PATH. Add it to $rc:"
	if [ "${rc%fish*}" != "$rc" ]; then
		say "    fish_add_path $INSTALL_DIR"
	else
		say "    export PATH=\"$INSTALL_DIR:\$PATH\""
	fi
	say ""
	say "Then:  notopod ~/notes"
	say ""
	;;
esac
