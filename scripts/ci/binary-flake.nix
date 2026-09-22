{
  description = "Prebuilt Lince from GitHub Actions";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        release = builtins.fromJSON (builtins.readFile ./binary.json);
        libraries = with pkgs; [
          stdenv.cc.cc.lib
          openssl
          sqlite
          libGL
          libdrm
          libgbm
          libx11
          libxcb
          libxcomposite
          libxdamage
          libxext
          libxfixes
          libxcursor
          libxi
          alsa-lib
          libxkbcommon
          libxrandr
          vulkan-loader
          wayland
        ];
        lince = pkgs.stdenvNoCC.mkDerivation {
          pname = "lince";
          version = "${release.version}-${builtins.substring 0 12 release.revision}";
          src = pkgs.fetchurl {
            inherit (release) url hash;
          };
          nativeBuildInputs = with pkgs; [
            autoPatchelfHook
            makeWrapper
          ];
          buildInputs = libraries;
          dontBuild = true;
          dontStrip = true;
          installPhase = ''
            runHook preInstall
            install -Dm755 bin/lince "$out/bin/lince"
            install -Dm644 LICENSE "$out/share/licenses/lince/LICENSE"
            install -Dm644 revision "$out/share/lince/revision"
            runHook postInstall
          '';
          postFixup = ''
            wrapProgram "$out/bin/lince" \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath libraries}"
          '';
          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            "$out/bin/lince" --help > /dev/null
            runHook postInstallCheck
          '';
          meta = {
            description = "Lince with its native interface and Cell runtime";
            mainProgram = "lince";
            license = pkgs.lib.licenses.mit;
            platforms = [ system ];
          };
        };
      in
      {
        packages = {
          default = lince;
          inherit lince;
          lince-ui = lince;
        };
        apps = {
          default = {
            type = "app";
            program = "${lince}/bin/lince";
            meta.description = lince.meta.description;
          };
          lince = self.apps.${system}.default;
        };
        checks.binary = lince;
      }
    )
    // {
      nixosModules.default =
        { pkgs, ... }:
        {
          imports = [ ./lince-module.nix ];
          services.lince.serverPackage =
            nixpkgs.lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.lince;
          services.lince.uiPackage =
            nixpkgs.lib.mkDefault
              self.packages.${pkgs.stdenv.hostPlatform.system}.lince-ui;
        };
      nixosModules.lince = self.nixosModules.default;
    };
}
