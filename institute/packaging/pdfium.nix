{ stdenv, stdenvNoCC, lib, fetchzip, autoPatchelfHook }:
let
  artifacts = {
    x86_64-linux = { name = "linux-x64"; hash = "sha256-Z/2osUULxUn6hOqB2HM/MqcoT/HGS0/YCKrAHfx2xoE="; };
    aarch64-linux = { name = "linux-arm64"; hash = "sha256-kPpN7wvGjKyaCav2NCI+OBNww0FFkkVEaL2RQY0sBcI="; };
    x86_64-darwin = { name = "mac-x64"; hash = "sha256-g/W30k/N07PNpxZcPPppGs2On+gYQ6Mp/hxs7BLFkEw="; };
    aarch64-darwin = { name = "mac-arm64"; hash = "sha256-vz7HTQfsmh9UKld1hAZDz1iEcI6Xp2RD+VbRcFebs7Y="; };
  };
  artifact = artifacts.${stdenv.hostPlatform.system};
in stdenvNoCC.mkDerivation {
  pname = "lince-pdfium";
  version = "7881";
  src = fetchzip {
    url = "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/pdfium-${artifact.name}.tgz";
    inherit (artifact) hash;
    stripRoot = false;
  };
  nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [ stdenv.cc.cc.lib ];
  installPhase = ''
    mkdir -p $out/lib $out/share/licenses/pdfium
    cp lib/* $out/lib/
    cp LICENSE $out/share/licenses/pdfium/
    cp -r licenses $out/share/licenses/pdfium/third-party
  '';
  meta = { license = lib.licenses.bsd3; platforms = builtins.attrNames artifacts; };
}
