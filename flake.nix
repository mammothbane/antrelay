{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/release-22.05";
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };

    naersk = {
      url = "git+ssh://gitea@git.nathanperry.dev/fork/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  description = "lunar relay";

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    ...
  } @ inputs: (flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        inherit system;

        overlays = [
          (import inputs.nixpkgs-mozilla)

          (self: super: let
            rust = (super.rustChannelOf {
              channel = "nightly";
              date = "2022-03-29";
              sha256 = "Ht5GU1xscXyhtc1zH/ppb2zJ259UXOvflcnfGdi9Adw=";
            }).rust.override {
              extensions = ["rust-src"];
              targets = [
                "x86_64-unknown-linux-musl"
              ];
            };
          in {
            cargo = rust;
            rustc = rust;

            naerskRust = rust;
          })
        ];
      };

      naersk = (inputs.naersk.lib."${system}".override {
        inherit (pkgs) cargo rustc;
      });

      deps = with pkgs; [
        pkgconfig
      ];

      inherit (pkgs) lib;

      pkg = naersk.buildPackage {
        pname = "lunarrelay";
        version = self.rev or "dirty";

        target = "x86_64-unknown-linux-musl";

        src = lib.cleanSourceWith (with builtins; {
          src = lib.cleanSource ./.;

          filter = let
            basePath = "${toString ./.}/";

          in path: type: let
            relPath = lib.removePrefix basePath path;
            result =
              relPath == "Cargo.lock" ||
              relPath == "Cargo.toml" ||
              relPath == "build.rs" ||
              relPath == "src" ||
              lib.hasPrefix "src/" relPath
              ;

          in result;

        });

        cargoBuildOptions = super: super ++ [
          "--target" "x86_64-unknown-linux-musl"
        ];

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
          cargo
          rustc

          (pkgs.writeShellScriptBin "sockpair" ''
            exec ${pkgs.socat}/bin/socat -d -d pty,raw,echo=0 pty,raw,echo=0
          '')
        ]) ++ deps;

        RUST_SRC_PATH = "${pkgs.naerskRust}/lib/rustlib/src/rust";

        RUST_BACKTRACE = "1";
      };

      packages = {
        bin = pkg;
        inherit docker;
      };

      defaultPackage = pkg;

      defaultApp = {
        type = "app";
        program = "${pkg}/bin/lunarrelay";
      };
    })
  );
}
