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

      pkgs = import nixpkgs {
        inherit system;

        overlays = [
          (import inputs.rust-overlay)

          (final: prev: let
            local_rust = prev.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          in {
            inherit local_rust;
            crane-lib = (inputs.crane.mkLib final).overrideToolchain local_rust;
            naersk = prev.callPackage inputs.naersk {
              cargo = local_rust;
              rustc = local_rust;
            };
          })
        ];
      };

      deps = with pkgs; [
        pkgconfig
      ];

      inherit (pkgs) lib;

      pkg = pkgs.crane-lib.buildPackage {
        pname = "antrelay";
        version = self.rev or "dirty";

        target = "x86_64-unknown-linux-musl";

        src = lib.cleanSourceWith (with builtins; {
          src = lib.cleanSource ./.;

          filter = let
            basePath = "${toString ./.}/";

          in path: type: let
            relPath = lib.removePrefix basePath path;

            isMember = builtins.any
              (member:
                relPath == member ||
                lib.hasPrefix member relPath ||
                lib.hasPrefix "${member}/" relPath)
              cargoToml.workspace.members;

            result =
              relPath == "Cargo.lock" ||
              relPath == "Cargo.toml" ||
              relPath == "build.rs" ||
              relPath == "src" ||
              lib.hasPrefix "src/" relPath ||
              isMember
              ;

          in result;

        });

        cargoExtraArgs = "--target" "x86_64-unknown-linux-musl";

        buildInputs = deps;
        remapPathPrefix = true;
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
            ${pkgs.upx}/bin/upx --best --ultra-brute -o relay.upx ${pkg}/bin/relay
          '';

          installPhase = ''
            install -d -m 0744 $out
            install -m 0700 relay.upx $out/relay
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
        ]) ++ deps;

        RUST_BACKTRACE = "1";

        shellHook = with pkgs; ''
          readonly ROOT=$(git rev-parse --show-toplevel)

          mkdir -p $ROOT/.devlinks

          rm -f $ROOT/.devlinks/rust

          ln -sf ${local_rust} $ROOT/.devlinks/rust
        '';
      };

      packages = {
        bin = pkg;
        inherit docker;
      };

      defaultPackage = pkg;

      defaultApp = {
        type = "app";
        program = "${pkg}/bin/antrelay";
      };
    })
  );
}
