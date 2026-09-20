#!/usr/bin/env bash
#
# Build a signed apt repository from a directory of .deb files.
#
# The result is a self-contained static tree, meant to be served from GitHub Pages alongside the
# rest of the site (see .github/workflows/pages.yml). Nothing here is committed to the repo: the
# pool is rebuilt from the .deb assets of the GitHub releases on every run, so the published
# repository is always a pure function of the releases that exist. Losing it costs one workflow
# run, not a recovery.
#
#   scripts/build_apt_repo.sh --debs <dir of .deb files> --out <output dir>
#
# The signing key comes from the environment, never from an argument (arguments show up in `ps`
# and in workflow logs):
#
#   APT_GPG_PRIVATE_KEY   required, ASCII-armoured private key (`gpg --armor --export-secret-keys`)
#   APT_GPG_PASSPHRASE    optional, only if that key has one
#
# See packaging/README.md for how to generate that key and load it as a repository secret.

set -euo pipefail

# Single suite, single component: this repository ships one package for one project, and a
# codename per Debian/Ubuntu release would mean rebuilding the same .deb under a dozen names
# without any of them differing. "stable main" is what third-party repositories with one stream
# conventionally use, and it is what the sources.list line in README.md names.
SUITE="stable"
COMPONENT="main"
ORIGIN="codediff"
LABEL="codediff"
DESCRIPTION="Unofficial codediff packages for Debian and Ubuntu"
# Bare name, no .gpg suffix on the key id: this is the file users fetch into /etc/apt/keyrings/.
KEYRING_NAME="codediff-archive-keyring.gpg"

debs_dir=""
out_dir=""
while [ $# -gt 0 ]; do
    case "$1" in
        --debs) debs_dir="${2:?--debs needs a directory}"; shift 2 ;;
        --out)  out_dir="${2:?--out needs a directory}"; shift 2 ;;
        *) echo "error: unknown argument '$1'" >&2; exit 2 ;;
    esac
done

[ -n "$debs_dir" ] && [ -n "$out_dir" ] || {
    echo "usage: $0 --debs <dir> --out <dir>" >&2
    exit 2
}
[ -d "$debs_dir" ] || { echo "error: no such directory: $debs_dir" >&2; exit 1; }
[ -n "${APT_GPG_PRIVATE_KEY:-}" ] || {
    echo "error: APT_GPG_PRIVATE_KEY is empty or unset" >&2
    exit 1
}

for tool in apt-ftparchive dpkg-deb gpg; do
    command -v "$tool" >/dev/null || { echo "error: $tool is not on PATH" >&2; exit 1; }
done

# An unsigned repository that looks signed is the one failure mode worth being loud about, so the
# key is imported into a throwaway keyring that is wiped on every exit path, and the run aborts if
# the import yields no secret key at all.
gnupg_home="$(mktemp -d)"
chmod 700 "$gnupg_home"
cleanup() {
    gpgconf --homedir "$gnupg_home" --kill all >/dev/null 2>&1 || true
    rm -rf "$gnupg_home"
}
trap cleanup EXIT
export GNUPGHOME="$gnupg_home"

printf '%s\n' "$APT_GPG_PRIVATE_KEY" | gpg --batch --quiet --import

# Field 5 of a `sec:` line is the long key id. `exit` after the first: a key with subkeys prints
# `sec` once, but an imported bundle of several keys would otherwise silently sign with all of
# them.
key_id="$(gpg --list-secret-keys --with-colons | awk -F: '/^sec:/ { print $5; exit }')"
[ -n "$key_id" ] || { echo "error: APT_GPG_PRIVATE_KEY holds no secret key" >&2; exit 1; }
echo "Signing with key $key_id"

gpg_sign=(gpg --batch --yes --quiet --local-user "$key_id" --digest-algo SHA256)
if [ -n "${APT_GPG_PASSPHRASE:-}" ]; then
    # Via a file inside the 0700 throwaway homedir rather than --passphrase: a command line is
    # readable by any process on the machine, and GitHub's log redaction does not reach `ps`.
    printf '%s' "$APT_GPG_PASSPHRASE" > "$gnupg_home/passphrase"
    chmod 600 "$gnupg_home/passphrase"
    gpg_sign+=(--pinentry-mode loopback --passphrase-file "$gnupg_home/passphrase")
fi

rm -rf "$out_dir"
mkdir -p "$out_dir"
# Every apt-ftparchive path below is relative on purpose: `Filename:` in Packages is written
# exactly as the directory was named on the command line, and an absolute path there produces a
# repository that only resolves on the machine that built it.
out_dir="$(cd "$out_dir" && pwd)"
debs_dir="$(cd "$debs_dir" && pwd)"

# Pool, laid out the way Debian archives are: pool/<component>/<prefix>/<source>/, where the
# prefix is the source package's first letter, or the first four characters for lib*.
deb_count=0
architectures=""
while IFS= read -r -d '' deb; do
    package="$(dpkg-deb -f "$deb" Package)"
    version="$(dpkg-deb -f "$deb" Version)"
    arch="$(dpkg-deb -f "$deb" Architecture)"
    [ -n "$package" ] && [ -n "$version" ] && [ -n "$arch" ] || {
        echo "error: $deb is missing Package/Version/Architecture" >&2
        exit 1
    }

    case "$package" in
        lib?*) prefix="${package:0:4}" ;;
        *)     prefix="${package:0:1}" ;;
    esac
    dest="$out_dir/pool/$COMPONENT/$prefix/$package"
    mkdir -p "$dest"

    # Canonical Debian filename, rebuilt from the control fields rather than taken from the
    # downloaded asset's name. The release assets are called codediff_amd64.deb and
    # codediff_arm64.deb with no version in them, so copying them across releases under their own
    # names would have every release overwrite the last and leave the pool holding one version.
    # An epoch is legal in a Version field and illegal in a filename.
    file_version="${version##*:}"
    cp -f "$deb" "$dest/${package}_${file_version}_${arch}.deb"

    case " $architectures " in
        *" $arch "*) ;;
        *) architectures="${architectures:+$architectures }$arch" ;;
    esac
    deb_count=$((deb_count + 1))
done < <(find "$debs_dir" -type f -name '*.deb' -print0)

[ "$deb_count" -gt 0 ] || { echo "error: no .deb files found in $debs_dir" >&2; exit 1; }
echo "Pooled $deb_count package file(s) for: $architectures"

cd "$out_dir"

# One Packages index per architecture actually present. Deriving the list from the pool rather
# than hardcoding it keeps the Release file honest: declaring an architecture with no index
# behind it makes `apt update` fail for exactly the users who need that architecture.
for arch in $architectures; do
    index_dir="dists/$SUITE/$COMPONENT/binary-$arch"
    mkdir -p "$index_dir"
    # --arch also picks up `all`, which is what a future arch-independent package would be.
    apt-ftparchive --arch "$arch" packages pool > "$index_dir/Packages"
    gzip -9 -c "$index_dir/Packages" > "$index_dir/Packages.gz"
    echo "  binary-$arch: $(grep -c '^Package: ' "$index_dir/Packages") package(s)"
done

# No Valid-Until. Adding one would make the repository expire on a date nobody is watching, and
# the tree is only rebuilt when something is pushed or released - a quiet month must not break
# `apt update` for everyone.
apt-ftparchive \
    -o "APT::FTPArchive::Release::Origin=$ORIGIN" \
    -o "APT::FTPArchive::Release::Label=$LABEL" \
    -o "APT::FTPArchive::Release::Suite=$SUITE" \
    -o "APT::FTPArchive::Release::Codename=$SUITE" \
    -o "APT::FTPArchive::Release::Components=$COMPONENT" \
    -o "APT::FTPArchive::Release::Architectures=$architectures" \
    -o "APT::FTPArchive::Release::Description=$DESCRIPTION" \
    release "dists/$SUITE" > "dists/$SUITE/Release"

# Both signatures, because both are still in use: apt prefers the inline InRelease and falls back
# to Release + Release.gpg on older clients.
"${gpg_sign[@]}" --clearsign --output "dists/$SUITE/InRelease" "dists/$SUITE/Release"
"${gpg_sign[@]}" --armor --detach-sign --output "dists/$SUITE/Release.gpg" "dists/$SUITE/Release"

# The public half, dearmoured, which is the form `signed-by=` wants in /etc/apt/keyrings/.
gpg --batch --yes --export --output "$KEYRING_NAME" "$key_id"

# Verify what was just written instead of trusting that gpg exited 0: gpgv against the exported
# public key is the same check apt itself performs, so a key that cannot verify its own signature
# fails here rather than on a user's machine.
gpgv --keyring "$out_dir/$KEYRING_NAME" "dists/$SUITE/InRelease" 2>/dev/null \
    || { echo "error: InRelease does not verify against $KEYRING_NAME" >&2; exit 1; }
gpgv --keyring "$out_dir/$KEYRING_NAME" "dists/$SUITE/Release.gpg" "dists/$SUITE/Release" 2>/dev/null \
    || { echo "error: Release.gpg does not verify against $KEYRING_NAME" >&2; exit 1; }

echo "Signed apt repository written to $out_dir (suite '$SUITE', component '$COMPONENT')"
