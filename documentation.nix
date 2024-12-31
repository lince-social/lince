{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    slides
    graph-easy
  ];
  shellHook = ''
    cd ${toString ./.}
  '';
}
