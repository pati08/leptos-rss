{
  pkgs,
  lib,
  crane,
}: let
  src = lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      ((lib.hasSuffix "\.html" path) ||
      (lib.hasSuffix "\.txt" path) ||
      (lib.hasSuffix "tailwind.config.js" path) ||
      (lib.hasInfix "/public/" path) ||
      (lib.hasInfix "/style/" path) ||
      (lib.hasInfix "/src/" path) ||
      (lib.hasInfix "/\.sqlx/" path) ||
      (craneLib.filterCargoSources path type))
      && !(
        (lib.hasInfix "/cargo_cache/" path) ||
        (lib.hasInfix "/target/" path)
      );
  };
  rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
    extensions = [ "rust-src" "rust-analyzer" "rustfmt" "rustc-codegen-cranelift-preview" ];
  };
  craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
  commonArgs = {
    inherit src;
    cargoExtraArgs = "--features ssr";
  };
  artifacts = craneLib.buildDepsOnly commonArgs;
  server = craneLib.buildPackage (commonArgs // {
    cargoArtifacts = artifacts;
  });
in server
