{
  description = "on-air — LAN audio streaming to AirPlay, Bluetooth, or Sonos";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      filteredSrc =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        pkgs.lib.cleanSourceWith {
          src = self;
          filter =
            path: type:
            let
              base = baseNameOf path;
            in
            !builtins.elem base [
              "target"
              "node_modules"
              ".git"
              ".turbo"
              "dist"
              "test-results"
              "result"
            ];
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          on-air-core = pkgs.callPackage ./packaging/nix/on-air-core.nix {
            src = filteredSrc system;
          };
        in
        {
          inherit on-air-core;
          default = on-air-core;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.on-air-core}/bin/on-air-core";
        };
        on-air-core = self.apps.${system}.default;
      });

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                rustc
                cargo
                rustfmt
                clippy
                pkg-config
                nodejs_22
                pnpm
                openssl
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
                alsa-lib
                gtk3
                webkitgtk_4_1
                libappindicator-gtk3
                librsvg
              ];
            shellHook = ''
              export PORT="''${PORT:-47990}"
            '';
          };
        }
      );

      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.services.on-air-core;
        in
        {
          options.services.on-air-core = {
            enable = lib.mkEnableOption "on-air-core LAN audio streaming service";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.on-air-core;
              description = "on-air-core package to run.";
            };
            port = lib.mkOption {
              type = lib.types.port;
              default = 47990;
              description = "HTTP/WebSocket listen port.";
            };
            mock = lib.mkEnableOption "mock senders (ON_AIR_MOCK=1)";
          };

          config = lib.mkIf cfg.enable {
            users.users.on-air-core = {
              isSystemUser = true;
              group = "on-air-core";
              extraGroups = lib.optionals (!cfg.mock) [ "audio" ];
            };
            users.groups.on-air-core = { };
            systemd.services.on-air-core = {
              description = "on-air-core (LAN audio streaming)";
              after = [ "network-online.target" ];
              wants = [ "network-online.target" ];
              wantedBy = [ "multi-user.target" ];
              environment = {
                PORT = toString cfg.port;
              } // lib.optionalAttrs cfg.mock { ON_AIR_MOCK = "1"; };
              serviceConfig = {
                ExecStart = "${cfg.package}/bin/on-air-core";
                Restart = "on-failure";
                User = "on-air-core";
                Group = "on-air-core";
              };
            };
          };
        };
    };
}
