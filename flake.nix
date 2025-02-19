{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        ./nix/flake-module.nix
      ];
      perSystem = { config, self', pkgs, lib, system, ... }: let
        binPkg = self'.packages.rss-chat;
        dockerImage = pkgs.dockerTools.buildImage {
          name = "rss-chat";
          tag = "latest";

          copyToRoot = [ binPkg ];
          config = {
            Env = [ "RUST_LOG=info" ];
            Entrypoint = [ "${binPkg}/rss-chat" ];
          };
        };
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
          ];
        };

        packages = {
          inherit binPkg dockerImage;
          default = binPkg;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            just
            irust
            leptosfmt
            cargo-leptos
            mold
            dart-sass
            tailwindcss
            binaryen
            (pkgs.rust-bin.nightly.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" "rustfmt" "rustc-codegen-cranelift-preview" ];
            })
          ];
        };
      };
    };
}
# {
#   description = "A development environment with nightly Rust, just, and irust";
#
#   inputs = {
#     nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
#     rust-overlay.url = "github:oxalica/rust-overlay";
#     flake-utils.url = "github:numtide/flake-utils";
#   };
#
#   outputs = { self, nixpkgs, rust-overlay, flake-utils }:
#     flake-utils.lib.eachDefaultSystem (system:
#       let
#         overlays = [ (import rust-overlay) ];
#         pkgs = import nixpkgs {
#           inherit system overlays;
#         };
#         rust-nightly = pkgs.rust-bin.nightly.latest.default.override {
#           extensions = [ "rust-src" "rust-analyzer" "rustfmt" "rustc-codegen-cranelift-preview" ];
#           targets = [ "x86_64-unknown-linux-gnu" "wasm32-unknown-unknown" ];
#         };
#       in
#       {
#         devShell = pkgs.mkShell {
#           buildInputs = [
#             rust-nightly
#             pkgs.just
#             pkgs.irust
#             pkgs.leptosfmt
#             pkgs.cargo-leptos
#             pkgs.mold
#             pkgs.dart-sass
#             pkgs.tailwindcss
#             pkgs.binaryen
#           ];
#
#           shellHook = ''
#             echo "Welcome to the Rust development environment!"
#             echo "Rust version: $(rustc --version)"
#             echo "Just version: $(just --version)"
#             echo "IRust version: $(irust --version)"
#             echo "Leptosfmt version: $(leptosfmt --version)"
#             echo "Cargo Leptos installed"
#             echo "TailwindCSS installed"
#           '';
#         };
#       }
#     );
# }
