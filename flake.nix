{
  description = "Lince";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      eachSystem = flake-utils.lib.eachDefaultSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };

          mkLince = { pname, features, noDefaultFeatures ? true }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname;
              version = "0.6.1";
              src = ./.;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              cargoBuildFlags =
                [ "--package" "lince" ]
                ++ pkgs.lib.optionals noDefaultFeatures [ "--no-default-features" ]
                ++ [ "--features" features ];

              cargoTestFlags =
                [ "--package" "lince" ]
                ++ pkgs.lib.optionals noDefaultFeatures [ "--no-default-features" ]
                ++ [ "--features" features ];

              nativeBuildInputs = with pkgs; [
                pkg-config
              ];

              buildInputs = with pkgs; [
                sqlite
              ];
            };

          lince-http = mkLince {
            pname = "lince-http";
            features = "http,karma";
          };

          lince-tui = mkLince {
            pname = "lince-tui";
            features = "tui,karma";
          };
        in {
          packages = {
            default = lince-http;
            inherit lince-http lince-tui;
          };

          apps = {
            default = {
              type = "app";
              program = "${lince-http}/bin/lince";
            };

            http = {
              type = "app";
              program = "${lince-http}/bin/lince";
            };

            tui = {
              type = "app";
              program = "${lince-tui}/bin/lince";
            };
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              pkg-config
              sqlite
            ];
          };
        });
    in
      eachSystem // {
        nixosConfigurations.manas-organ = nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          specialArgs = { inherit self; };
          modules = [
            ./nixos/configuration.nix
          ];
        };
      };
}
