{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [ nodejs_23 ];
  shellHook = ''
    cd ${toString ./.}
    nvim ${toString ./.}
  '';
}
