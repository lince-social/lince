{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [ devenv ];
  shellHook = ''test -f `pwd`/devenv.nix && devenv shell || devenv init'';
}
