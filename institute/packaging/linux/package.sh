#!/usr/bin/env bash
set -euo pipefail
[[ $# == 2 ]] || { echo 'Usage: package.sh EXECUTABLE DESTINATION.tar.gz' >&2; exit 2; }
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
executable=$(realpath "$1")
destination=$(realpath -m "$2")
[[ ! -e $(dirname "$executable")/.lince-wrapped ]] || { echo 'Package the compiled ELF before Nix wrapping' >&2; exit 1; }
[[ $(od -An -tx1 -N4 "$executable" | tr -d ' \n') == 7f454c46 ]] || { echo 'Expected an ELF executable' >&2; exit 1; }
mkdir -p "$(dirname "$destination")"
temporary=$(mktemp -d "$(dirname "$destination")/park-package-XXXXXX")
trap 'rm -rf -- "$temporary"' EXIT
root=$temporary/lince
mkdir -p "$root"
timeout 10 "$executable" --build-info > "$root/build-info.json"
bash "$repository/institute/packaging/linux/check-build-info.sh" "$root/build-info.json"
install -Dm755 "$executable" "$root/bin/lince"
for mapping in \
    'LICENSE:LICENSE' \
    'institute/packaging/linux/wayland-sessions/lince.desktop:share/wayland-sessions/lince.desktop' \
    'crates/vendor/smithay-capture/LICENSE.txt:share/licenses/lince-smithay-capture/LICENSE.txt' \
    'crates/vendor/smithay-capture/CREDITS.md:share/licenses/lince-smithay-capture/CREDITS.md' \
    'institute/packaging/linux/ufw/lince:etc/ufw/applications.d/lince' \
    'institute/packaging/linux/firewalld/lince.xml:lib/firewalld/services/lince.xml' \
    'institute/packaging/linux/firewalld/lince-test.xml:lib/firewalld/services/lince-test.xml'; do
    install -Dm644 "$repository/${mapping%%:*}" "$root/${mapping#*:}"
done
install -Dm755 "$repository/institute/packaging/linux/lince-park-session" "$root/bin/lince-park-session"
jq -r .revision "$root/build-info.json" > "$root/revision"
tar -I 'gzip -1' -cf "$temporary/archive.tar.gz" -C "$temporary" lince
mv "$temporary/archive.tar.gz" "$destination"
printf '%s  %s\n' "$(sha256sum "$destination" | cut -d ' ' -f1)" "$(basename "$destination")" > "$destination.sha256"
jq -r .target "$root/build-info.json" > "$destination.target"
