{ pkgs, graphicsWrapperArgs }:
let
  launcher = pkgs.runCommand "park-launch-fixture" {
    nativeBuildInputs = [ pkgs.makeWrapper ];
  } ''
    mkdir -p "$out/bin"
    cat > "$out/bin/lince" <<'PROGRAM'
    #!${pkgs.runtimeShell}
    if [ "''${1-}" = --graphics ]; then
      printf '%s\n' "$LD_LIBRARY_PATH" "$GBM_BACKENDS_PATH" "$__EGL_VENDOR_LIBRARY_DIRS"
      exit 0
    fi
    printf '%s\n' "$LINCE_HOST_LD_LIBRARY_PATH" "$LINCE_HOST_GBM_BACKENDS_PATH" "$LINCE_HOST___EGL_VENDOR_LIBRARY_DIRS"
    printf '%s\n' "$@"
    test -n "$DBUS_SESSION_BUS_ADDRESS"
    PROGRAM
    chmod +x "$out/bin/lince"
    wrapProgram "$out/bin/lince" ${pkgs.lib.escapeShellArgs (graphicsWrapperArgs pkgs [ pkgs.stdenv.cc.cc.lib ])}
    cp ${./lince-park-session} "$out/bin/lince-park-session"
    substituteInPlace "$out/bin/lince-park-session" --replace-fail \
      'dbus-run-session -- ' 'dbus-run-session --config-file=${pkgs.dbus}/share/dbus-1/session.conf -- '
    chmod +x "$out/bin/lince-park-session"
    patchShebangs "$out/bin/lince-park-session"
  '';
in pkgs.runCommand "park-launch-contracts" {
  nativeBuildInputs = [ pkgs.dbus pkgs.coreutils ];
} ''
  env -u LD_LIBRARY_PATH -u GBM_BACKENDS_PATH -u __EGL_VENDOR_LIBRARY_DIRS \
    DBUS_SESSION_BUS_ADDRESS=existing-bus ${launcher}/bin/lince-park-session 'literal $(touch unexpected)' > unset
  printf '%s\n' 0 0 0 --park 'literal $(touch unexpected)' > expected
  cmp unset expected
  env -u DBUS_SESSION_BUS_ADDRESS LD_LIBRARY_PATH= GBM_BACKENDS_PATH='host $value' __EGL_VENDOR_LIBRARY_DIRS='a:b c' \
    ${launcher}/bin/lince-park-session --directory 'a b' > overridden
  printf '%s\n' 1 '1host $value' '1a:b c' --park --directory 'a b' > expected
  cmp overridden expected
  env LD_LIBRARY_PATH=/outer/private GBM_BACKENDS_PATH=/outer/gbm __EGL_VENDOR_LIBRARY_DIRS=/outer/egl \
    LINCE_HOST_LD_LIBRARY_PATH=0 LINCE_HOST_GBM_BACKENDS_PATH=0 LINCE_HOST___EGL_VENDOR_LIBRARY_DIRS=0 \
    ${launcher}/bin/lince --graphics > normalized-unset
  printf '%s\n' '${pkgs.lib.makeLibraryPath [ pkgs.stdenv.cc.cc.lib ]}' '${pkgs.mesa}/lib/gbm' '${pkgs.mesa}/share/glvnd/egl_vendor.d' > expected
  cmp normalized-unset expected
  env LD_LIBRARY_PATH=/outer/private GBM_BACKENDS_PATH=/outer/gbm __EGL_VENDOR_LIBRARY_DIRS=/outer/egl \
    LINCE_HOST_LD_LIBRARY_PATH=1host-original LINCE_HOST_GBM_BACKENDS_PATH=1 LINCE_HOST___EGL_VENDOR_LIBRARY_DIRS='1host egl' \
    ${launcher}/bin/lince --graphics > normalized-overrides
  printf '%s\n' '${pkgs.lib.makeLibraryPath [ pkgs.stdenv.cc.cc.lib ]}:host-original' "" 'host egl' > expected
  cmp normalized-overrides expected
  test ! -e unexpected
  mkdir -p "$out"
  cp unset overridden normalized-unset normalized-overrides "$out/"
''
