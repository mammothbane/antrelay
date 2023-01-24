{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-22.11";
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay/master";

      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    crane = {
      url = "github:ipetkov/crane";

      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
        rust-overlay.follows = "rust-overlay";
      };
    };

    cobs-python = {
      url = github:cmcqueen/cobs-python;
      flake = false;
    };
  };

  description = "relay for astroant";

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    ...
  } @ inputs: (flake-utils.lib.eachDefaultSystem (system:
    let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      overlays = [
        (import inputs.rust-overlay)

        (final: prev: let
          local_rust = prev.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in {
          inherit local_rust;
          crane-lib = (inputs.crane.mkLib final).overrideToolchain local_rust;
        })
      ];

      pkgs = import nixpkgs {
        inherit system overlays;
      };

      inherit (pkgs) lib;

      mk_pkg = target: cross_pkgs: with cross_pkgs; let
        toolchain = cross_pkgs.pkgsBuildHost.local_rust.override {
          targets = [ target ];
        };

        crane-lib-cross = (inputs.crane.mkLib cross_pkgs).overrideToolchain toolchain;

      in cross_pkgs.callPackage ./. {
        crane-lib = crane-lib-cross;
        target = target;
      };

      docker = pkgs.dockerTools.buildImage {
        name = "relay";
        tag = "latest";

        fromImage = pkgs.dockerTools.pullImage {
          imageName = "gcr.io/distroless/static";
          imageDigest = "sha256:d6fa9db9548b5772860fecddb11d84f9ebd7e0321c0cb3c02870402680cc315f";
          sha256 = "1f2rfkppk4y1szi8f7yhyikqffxv6fscf5kp6yz1f1c42scfibpi";
          finalImageName = "gcr.io/distroless/static";
          finalImageTag = "latest";
        };

        contents = pkgs.stdenv.mkDerivation {
          name = "relay-root-upx";
          phases = [ "buildPhase" "installPhase" ];

          buildPhase = ''
            ${pkgs.upx}/bin/upx --best --ultra-brute -o relay.upx ${self.packages.bin}/bin/antrelay
          '';

          installPhase = ''
            install -d -m 0744 $out
            install -m 0700 relay.upx $out/antrelay
          '';
        };

        runAsRoot = ''
          #!${pkgs.runtimeShell}
          ln -sf /usr/share/zoneinfo/UTC /etc/localtime
          echo 'UTC' > /etc/timezone
        '';

        config = {
          Entrypoint = [ "/relay" ];
          Cmd = [];
          WorkingDir = "/";
          Env = [
            "TZ=\"UTC\""
          ];
        };
      };

    in {
      devShell = pkgs.mkShell {
        buildInputs = (with pkgs; [
          local_rust
          pkgconfig

          (pkgs.writeShellScriptBin "sockpair" ''
            exec ${pkgs.socat}/bin/socat -d -d pty,raw,echo=0 pty,raw,echo=0
          '')

          (pkgs.python3.withPackages (p: with p; [
            pyserial

            (buildPythonPackage {
              pname = "cobs";
              version = "1.2.0";

              src = inputs.cobs-python;
            })
          ]))
        ]);

        RUST_BACKTRACE = "1";

        shellHook = with pkgs; ''
          readonly ROOT=$(git rev-parse --show-toplevel)

          mkdir -p $ROOT/.devlinks

          rm -f $ROOT/.devlinks/rust

          ln -sf ${local_rust} $ROOT/.devlinks/rust
        '';
      };

      packages = {
        default = self.packages.bin;
        bin = mk_pkg "x86_64-unknown-linux-musl" pkgs.pkgsStatic;
        aarch = mk_pkg "aarch64-unknown-linux-musl" pkgs.pkgsCross.aarch64-multiplatform-musl.pkgsStatic;
        inherit docker;
      };

      legacyPackages = pkgs;

      defaultApp = {
        type = "app";
        program = "${self.packages.bin}/bin/antrelay";
      };
    })
  ) // {
    nixosModules.default = { lib, pkgs, config, ... }: let
      cfg = config.services.antrelay;
    in {
      options = with lib; {
        services.antrelay = {
          enable = mkEnableOption "antrelay";

          package = mkOption {
            type = types.package;
            default = self.packages.${pkgs.system}.default;
          };

          serial_port = mkOption {
            type = types.path;
            default = "/dev/ttyUSB0";
          };

          baud = mkOption {
            type = types.int;
            default = 115200;
          };

          uplink = mkOption {
            type = types.path;
            default = "sockets/uplink";
          };

          downlink = mkOption {
            type = types.path;
            default = "sockets/downlink";
          };
        };
      };

      config = lib.mkIf cfg.enable {
        systemd.services.antrelay = {
          description = "antrelay service";

          wantedBy = [
            "multi-user.target"
          ];

          requires = [
            "basic.target"
          ];

          after = [
            "basic.target"
          ];

          unitConfig = {
            StartLimitBurst = 3;
            StartLimitIntervalSec = "1m";
          };

          serviceConfig = {
            DynamicUser = true;

            SupplementaryGroups = "dialout";

            StandardInput = "null";
            ExecStart = "${cfg.package}/bin/antrelay -s ${cfg.serial_port} -b ${toString cfg.baud} --uplink ${cfg.uplink} --downlink ${cfg.downlink}";

            Restart = "always";
            RestartSec = "10s";

            TimeoutStopSec = "5s";

            MemoryHigh = "80M";
            MemoryMax = "100M";
          };
        };
      };
    };
  };
}
