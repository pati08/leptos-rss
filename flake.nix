{
  description = "Leptos SSR Project Build";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };

      rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
        extensions = [ "rust-src" "rust-analyzer" "rustfmt" "rustc-codegen-cranelift-preview" ];
      };
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
      tomlNameInfo = craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };
      cargoPkgName = tomlNameInfo.pname;
      inherit (tomlNameInfo) version;

      server = targetPkgs: targetPkgs.callPackage ./nix/serverBuild.nix { inherit crane; };
      wasm = pkgs.callPackage ./nix/wasmBuild.nix { inherit crane; };
      # Build the site directory 
      siteDerivation = pkgs.stdenv.mkDerivation {
        name = "${cargoPkgName}-site";
        src = self;

        buildInputs = with pkgs; [
          tailwindcss
          wasm-bindgen-cli
          binaryen
        ];

        configurePhase = ''
          mkdir -p $out/target/site/pkg
          cp -r ${./.}/public/* $out/target/site/
        '';

        buildPhase = ''
          # Build Tailwind CSS
          tailwindcss -i ./style/tailwind.css -o $out/target/site/pkg/${cargoPkgName}-${version}.css
          
          # Process WASM
          wasm-bindgen --target web --out-dir $out/target/site/pkg \
            --reference-types \
            ${wasm}/bin/${cargoPkgName}.wasm

          wasm-opt -Oz -o $out/target/site/pkg/${cargoPkgName}-${version}_bg.wasm \
            $out/target/site/pkg/${cargoPkgName}_bg.wasm

          mv $out/target/site/pkg/${cargoPkgName}.js $out/target/site/pkg/${cargoPkgName}-${version}.js
        '';
      };

      dockerImage = targetPkgs: targetPkgs.dockerTools.buildImage {
        name = cargoPkgName;
        tag = "latest";

        copyToRoot = [ (server targetPkgs) siteDerivation ];

        config = {
          Cmd = [ "${server targetPkgs}/bin/${cargoPkgName}" ];
          Env = [
            "LEPTOS_SITE_DIR=target/site"
            "LEPTOS_SITE_ADDR=0.0.0.0:3000"
            "LEPTOS_OUTPUT_NAME=${cargoPkgName}-${version}"
            "NIX_BUILD=1"
          ];
        };
      };

      crossPkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        crossSystem.config = "aarch64-unknown-linux-gnu";
      };

    in {
      packages.${system} = {
        default = dockerImage pkgs;
        arm = dockerImage crossPkgs;
      };

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rustToolchain
          tailwindcss
          wasm-bindgen-cli
          binaryen
          just
          irust
          leptosfmt
          mold
          dart-sass
          tailwindcss
          dive
          cargo-leptos
        ];
      };
    };
}
