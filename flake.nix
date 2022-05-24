{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/release-21.11";
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };

    naersk = {
      url = "git+ssh://gitea@git.nathanperry.dev/fork/naersk";
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

        src = lib.cleanSourceWith (with builtins; {
          src = lib.cleanSource ./.;

          filter = let
            basePath = "${toString ./.}/";

          in path: type: let
            relPath = lib.removePrefix basePath path;
            result =
              relPath == "Cargo.lock" ||
              relPath == "Cargo.toml" ||
              relPath == "src" ||
              lib.hasPrefix "src/" relPath
              ;

          in result;
        });

        buildInputs = deps;
        remapPathPrefix = true;
      };

    in {
      devShell = pkgs.mkShell {
        buildInputs = (with pkgs; [
          cargo
          rustc
        ]) ++ deps;

        RUST_SRC_PATH = "${pkgs.naerskRust}/lib/rustlib/src/rust";

        RUST_BACKTRACE = "1";
      };

      defaultPackage = pkg;

      defaultApp = {
        type = "app";
        program = "${pkg}/bin/lunarrelay";
      };
    })
  );
}
