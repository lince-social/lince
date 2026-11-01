set -euo pipefail
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  mold \
  pigz \
  pkg-config \
  meson \
  ninja-build \
  curl \
  libfontconfig-dev \
  clang \
  libclang-dev \
  libpipewire-0.3-dev \
  libdbus-1-dev \
  libglib2.0-dev \
  libpulse-dev \
  libspeechd-dev \
  libva-dev \
  libssl-dev \
  libsqlite3-dev \
  libasound2-dev \
  libwayland-dev \
  libxkbcommon-dev \
  libx11-dev \
  libxcb1-dev \
  libxrandr-dev \
  libxi-dev \
  libxcursor-dev \
  libgl1-mesa-dev \
  libvulkan-dev \
  libegl1-mesa-dev libgbm-dev libdrm-dev libudev-dev libinput-dev libseat-dev
curl --fail --location --retry 3 --output /tmp/libdisplay-info-0.4.0.tar.xz \
  https://gitlab.freedesktop.org/emersion/libdisplay-info/-/releases/0.4.0/downloads/libdisplay-info-0.4.0.tar.xz
printf '%s\n' '43b180baa143e2035654759d84e2b2f5ee77d5fe817c423838c7fe59c0d68459  /tmp/libdisplay-info-0.4.0.tar.xz' | sha256sum --check
tar -xf /tmp/libdisplay-info-0.4.0.tar.xz -C /tmp
meson setup /tmp/libdisplay-info-build /tmp/libdisplay-info-0.4.0 --prefix=/usr/local --buildtype=release
sudo meson install -C /tmp/libdisplay-info-build
sudo ldconfig
pkg-config --atleast-version=0.4.0 libdisplay-info
case "${LINCE_CI_LINKER:-mold}" in
  mold) rustflags='-D warnings -C link-arg=-fuse-ld=mold' ;;
  wild)
    destination="$HOME/.local/bin"
    mkdir -p "$destination"
    archive="$RUNNER_TEMP/wild-0.9.0.tar.gz"
    curl --fail --location --retry 3 --output "$archive" https://github.com/wild-linker/wild/releases/download/0.9.0/wild-linker-0.9.0-x86_64-unknown-linux-gnu.tar.gz
    printf '%s  %s\n' deb6ee0e5caec798053ec4aafaba042e20a8edf91f08cb4d36268571cc628d3b "$archive" | sha256sum --check
    directory="$RUNNER_TEMP/wild-0.9.0"
    mkdir -p "$directory"
    tar -xzf "$archive" -C "$directory"
    binary="$(find "$directory" -type f -name wild -print -quit)"
    test -n "$binary"
    install -m755 "$binary" "$destination/wild"
    rustflags="-D warnings -C linker=clang -C link-arg=-fuse-ld=$destination/wild"
    ;;
  *) echo 'Unsupported Linux linker' >&2; exit 1 ;;
esac
printf 'RUSTFLAGS=%s\n' "$rustflags" >> "$GITHUB_ENV"
dpkg-query -W > "$RUNNER_TEMP/lince-native-inputs"
if [ "${LINCE_CI_LINKER:-mold}" = wild ]; then
  "$HOME/.local/bin/wild" --version >> "$RUNNER_TEMP/lince-native-inputs"
fi
