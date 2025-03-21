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
      inherit (pkgs) lib;

      tomlNameInfo = craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; };
      cargoPkgName = tomlNameInfo.pname;
      inherit (tomlNameInfo) version;

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
        targets = [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ];
      };
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

      commonArgs = {
        inherit src;
      };

      serverArgs = commonArgs // {
        cargoExtraArgs = "--features ssr";
      };
      wasmArgs = commonArgs // {
        cargoExtraArgs = "--target wasm32-unknown-unknown --features hydrate";
        doCheck = false;
      };
      serverArtifacts = craneLib.buildDepsOnly serverArgs;
      wasmArtifacts = craneLib.buildDepsOnly wasmArgs;

      # Build server binary with SSR feature
      server = craneLib.buildPackage (serverArgs // {
        cargoArtifacts = serverArtifacts;
      });
      # Build WASM with hydrate feature
      wasm = craneLib.buildPackage (wasmArgs // {
        cargoArtifacts = wasmArtifacts;
      });
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

      dockerImage = pkgs.dockerTools.buildImage {
        name = cargoPkgName;
        tag = "latest";

        copyToRoot = [ server siteDerivation ];

        config = {
          Cmd = [ "${server}/bin/${cargoPkgName}" ];
          Env = [
            "LEPTOS_SITE_DIR=target/site"
            "LEPTOS_SITE_ADDR=0.0.0.0:3000"
            "LEPTOS_OUTPUT_NAME=${cargoPkgName}-${version}"
            "NIX_BUILD=1"
          ];
        };
      };

    in {
      packages.${system} = {
        default = dockerImage;

        inherit server;
        inherit wasm;
        inherit dockerImage;
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
