{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    nodejs_23
    postgresql_17
    lua
    jq
  ];
  shellHook = ''
    cd ${toString ./.}
    nvim
  '';
}
