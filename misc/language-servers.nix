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
    nodePackages_latest.prettier
    nil
    lazygit
    eslint
    nixfmt-rfc-style
  ];
  shellHook = ''
    cd ${toString ../.}
    nvim
  '';
}
