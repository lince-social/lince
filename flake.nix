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
          cargoToml = builtins.fromTOML (builtins.readFile ./crates/lince/Cargo.toml);
          version = cargoToml.package.version;

          mkLince = {
            pname,
            features,
            noDefaultFeatures ? true,
          }:
            let
              featureList =
                if builtins.isList features then features else [ features ];
              cargoFlags =
                [ "--package" "lince" ]
                ++ lib.optionals noDefaultFeatures [ "--no-default-features" ]
                ++ [ "--features" (lib.concatStringsSep "," featureList) ];
            in
            pkgs.rustPlatform.buildRustPackage {
              inherit pname version;
              src = ./.;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              RUSTFLAGS = "-D warnings";

              cargoBuildFlags = cargoFlags;
              cargoTestFlags = cargoFlags;

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
                description =
                  "Lince binary built with ${lib.concatStringsSep ", " featureList} features";
                mainProgram = "lince";
                license = lib.licenses.gpl3Plus;
                platforms = supportedSystems;
              };
            };

          lince = mkLince {
            pname = "lince";
            features = [ "http" "karma" ];
          };

          lince-http = mkLince {
            pname = "lince-http";
            features = [ "http" "karma" ];
          };

          lince-tui = mkLince {
            pname = "lince-tui";
            features = [ "tui" "karma" ];
          };
        in
        {
          packages = {
            default = lince;
            inherit lince lince-http lince-tui;
          };

          apps = {
            default = flake-utils.lib.mkApp {
              drv = lince;
            };

            lince = flake-utils.lib.mkApp {
              drv = lince;
            };

            http = flake-utils.lib.mkApp {
              drv = lince-http;
            };

            tui = flake-utils.lib.mkApp {
              drv = lince-tui;
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
        });
    in
    eachSystem
    // {
      nixosConfigurations.manas-organ = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit self; };
        modules = [
          ./run/nixos/configuration.nix
        ];
      };
    };
}
