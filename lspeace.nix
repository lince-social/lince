{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  buildInputs = with pkgs; [
    postgresql_17
    sqls
    tailwindcss-language-server
    vscode-langservers-extracted
    typescript-language-server
    nil
  ];
  shellHook = ''
    cd ${toString ./.}
    nvim
  '';
}
