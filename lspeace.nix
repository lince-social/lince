{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    nodejs_23
    postgresql_17
    lua
  ];
  shellHook = ''
    cd ${toString ./.}
    nvim ${toString ./.}
  '';
}
