{
  stdenv,
  crane-lib,
  pkgconfig,
  target,
}:
let
  linkerArg = "-C linker=${stdenv.cc.targetPrefix}cc";

  src = crane-lib.cleanCargoSource ./.;

  nativeBuildInputs = [
    pkgconfig
  ];

  buildInputs = [
  ];

  commonOptions = {
    inherit src buildInputs nativeBuildInputs;
    CARGO_BUILD_TARGET = target;
    CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static ${linkerArg}";
    HOST_CC = "${stdenv.cc.nativePrefix}cc";
  };

  cargoArtifacts = crane-lib.buildDepsOnly (commonOptions // {
  });

in crane-lib.buildPackage (commonOptions // {
  inherit
    cargoArtifacts
    ;
})
