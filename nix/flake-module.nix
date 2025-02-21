{ self, lib, inputs, flake-parts-lib, ... }:

let
  inherit (flake-parts-lib)
    mkPerSystemOption;
in
{
  options = {
    perSystem = mkPerSystemOption
      ({ config, self', inputs', pkgs, system, ... }: {
        options = {
          rss-chat.overrideCraneArgs = lib.mkOption {
            type = lib.types.functionTo lib.types.attrs;
            default = _: { };
            description = "Override crane args for the rss-chat package";
          };

          rss-chat.rustToolchain = lib.mkOption {
            type = lib.types.package;
            description = "Rust toolchain to use for the rss-chat package";
            default = (pkgs.rust-bin.nightly.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" "rustfmt" "rustc-codegen-cranelift-preview" ];
            });
          };

          rss-chat.craneLib = lib.mkOption {
            type = lib.types.lazyAttrsOf lib.types.raw;
            default = (inputs.crane.mkLib pkgs).overrideToolchain config.rss-chat.rustToolchain;
          };

          rss-chat.src = lib.mkOption {
            type = lib.types.path;
            description = "Source directory for the rss-chat package";
            # When filtering sources, we want to allow assets other than .rs files
            # TODO: Don't hardcode these!
            default = lib.cleanSourceWith {
              src = self; # The original, unfiltered source
              filter = path: type:
                ((lib.hasSuffix "\.html" path) ||
                (lib.hasSuffix "\.txt" path) ||
                (lib.hasSuffix "\.toml" path) ||
                # (lib.hasSuffix "\.env" path) ||
                (lib.hasSuffix "tailwind.config.js" path) ||
                # Example of a folder for images, icons, etc
                (lib.hasInfix "/public/" path) ||
                (lib.hasInfix "/style/" path) ||
                (lib.hasInfix "/css/" path) ||
                (lib.hasInfix "/\.sqlx/" path) ||
                # Default filter from crane (allow .rs files)
                (config.rss-chat.craneLib.filterCargoSources path type))
                && !(
                  (lib.hasInfix "/cargo_cache/" path) ||
                  (lib.hasInfix "/target/" path)
                )
              ;
            };
          };
        };
        config =
          let
            cargoToml = builtins.fromTOML (builtins.readFile (self + /Cargo.toml));
            inherit (cargoToml.package) name version;
            inherit (config.rss-chat) rustToolchain craneLib src;

            # Crane builder for cargo-leptos projects
            craneBuild = rec {
              args = {
                inherit src;
                pname = name;
                version = version;
                buildInputs = [
                  (pkgs.callPackage (import ../cargo-leptos.nix) {})
                  pkgs.binaryen # Provides wasm-opt
                  tailwindcss
                  pkgs.dart-sass
                ];
              };
              cargoArtifacts = craneLib.buildDepsOnly args;
              buildArgs = args // {
                inherit cargoArtifacts;
                buildPhaseCargoCommand = "cargo leptos build --release -vvv";
                cargoTestCommand = "cargo leptos test --release -vvv";
                cargoExtraArgs = "";
                nativeBuildInputs = [
                  pkgs.makeWrapper
                ];
                installPhaseCommand = /* bash */ ''
                  mkdir -p $out/target
                  cp target/release/${name} $out/
                  cp -r target/site $out/target/
                  mkdir $out/assets
                  cp assets/tailwind.css $out/assets/
                  wrapProgram $out/${name} \
                    --set LEPTOS_SITE_ROOT $out/target/site
                '';
                doCheck = false;
              };
              package = craneLib.buildPackage (buildArgs // config.rss-chat.overrideCraneArgs buildArgs);

              check = craneLib.cargoClippy (args // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets --all-features -- --deny warnings";
              });

              doc = craneLib.cargoDoc (args // {
                inherit cargoArtifacts;
              });
            };

            rustDevShell = pkgs.mkShell {
              shellHook = ''
                # For rust-analyzer 'hover' tooltips to work.
                export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library";
              '';
              buildInputs = [
                pkgs.libiconv
              ];
              nativeBuildInputs = [
                rustToolchain
              ];
            };

            tailwindcss = pkgs.nodePackages.tailwindcss.overrideAttrs
              (oa: {
                plugins = [
                  pkgs.nodePackages."@tailwindcss/aspect-ratio"
                  pkgs.nodePackages."@tailwindcss/forms"
                  pkgs.nodePackages."@tailwindcss/language-server"
                  pkgs.nodePackages."@tailwindcss/line-clamp"
                  pkgs.nodePackages."@tailwindcss/typography"
                ];
              });
          in
          {
            # Rust package
            packages.${name} = craneBuild.package;
            packages."${name}-doc" = craneBuild.doc;

            checks."${name}-clippy" = craneBuild.check;

            # Rust dev environment
            devShells.${name} = pkgs.mkShell {
              inputsFrom = [
                rustDevShell
              ];
              nativeBuildInputs = with pkgs; [
                tailwindcss
                cargo-leptos
                binaryen # Provides wasm-opt
              ];
            };
          };
      });
  };
}
