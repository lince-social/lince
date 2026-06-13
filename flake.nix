{
  description = "Lince";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      eachSystem = flake-utils.lib.eachSystem supportedSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          lib = pkgs.lib;
          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          version = cargoToml.workspace.package.version;
          cleanSrc = lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let
                name = baseNameOf path;
                rel = lib.removePrefix (toString ./. + "/") (toString path);
              in
              !(
                name == ".direnv"
                || name == ".devenv"
                || name == ".git"
                || name == "mprocs.log"
                || name == "target"
                || lib.hasPrefix "target/" rel
              );
          };
          tauriLinuxNativeBuildInputs = with pkgs; lib.optionals stdenv.isLinux [
            pkg-config
            wrapGAppsHook3
          ];
          tauriLinuxBuildInputs = with pkgs; lib.optionals stdenv.isLinux [
            glib-networking
            gtk3
            libayatana-appindicator
            libxkbcommon
            librsvg
            libsoup_3
            webkitgtk_4_1
            xdotool
          ];

          mkLince = { pname }: pkgs.rustPlatform.buildRustPackage {
              inherit pname version;
              src = cleanSrc;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              RUSTFLAGS = "-D warnings";

              cargoBuildFlags = [ "--package" "lince" ];
              cargoTestFlags = [ "--package" "lince" ];

              nativeBuildInputs = with pkgs; [
                pkg-config
              ];

              buildInputs =
                with pkgs;
                [
                  openssl
                  sqlite
                ];

              meta = {
                description = "Lince binary";
                mainProgram = "lince";
                license = lib.licenses.gpl3Plus;
                platforms = supportedSystems;
              };
            };

          lince = mkLince {
            pname = "lince";
          };

          lince-desktop = pkgs.rustPlatform.buildRustPackage {
            pname = "lince-desktop";
            inherit version;
            src = cleanSrc;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            RUSTFLAGS = "-D warnings";

            cargoBuildFlags = [ "--package" "lince-desktop" ];
            cargoTestFlags = [ "--package" "lince-desktop" ];

            nativeBuildInputs = with pkgs; [
              pkg-config
            ] ++ lib.remove pkg-config tauriLinuxNativeBuildInputs;

            buildInputs =
              (with pkgs; [
                openssl
                sqlite
              ])
              ++ tauriLinuxBuildInputs;

            meta = {
              description = "Lince desktop webview application";
              mainProgram = "lince-desktop";
              license = lib.licenses.gpl3Plus;
              platforms = supportedSystems;
            };
          };
        in
        {
          packages = {
            default = lince;
            inherit lince lince-desktop;
          };

          apps = {
            default = flake-utils.lib.mkApp {
              drv = lince;
            };

            lince = flake-utils.lib.mkApp {
              drv = lince;
            };

            lince-desktop = flake-utils.lib.mkApp {
              drv = lince-desktop;
            };
          };

          formatter = pkgs.nixfmt-rfc-style;

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              openssl
              pkg-config
              rust-analyzer
              rustc
              rustfmt
              sqlite
            ];
          };

          devShells.desktop = pkgs.mkShell {
            packages =
              (with pkgs; [
                cargo
                cargo-tauri
                clippy
                openssl
                pkg-config
                rust-analyzer
                rustc
                rustfmt
                sqlite
              ])
              ++ tauriLinuxNativeBuildInputs
              ++ tauriLinuxBuildInputs;

            shellHook = ''
              export RUSTFLAGS="-D warnings"
            '' + lib.optionalString pkgs.stdenv.isLinux ''
              export LD_LIBRARY_PATH="${lib.makeLibraryPath (tauriLinuxBuildInputs ++ (with pkgs; [ openssl sqlite ]))}:''${LD_LIBRARY_PATH:-}"
              export WEBKIT_DISABLE_COMPOSITING_MODE=1
            '';
          };
        });
    in
    eachSystem
    // {
      nixosConfigurations.manas-organ = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit self; };
        modules = [
          ./scripts/deploy/nixos/configuration.nix
        ];
      };
    };
}
