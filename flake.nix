{
  description = "Aerogramme";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };

    cargo2nix = {
      url = "github:cargo2nix/cargo2nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
        rust-overlay.follows = "rust-overlay";
      };
    };

    albatros.url = "git+https://git.deuxfleurs.fr/Deuxfleurs/albatros.git?ref=main";
  };

  outputs = { self, albatros, nixpkgs, cargo2nix, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem(system:
      let
        ###
        #
        # --- INITIALIZATION ---
        #
        ###
        pkgs = import nixpkgs { 
          system = system;
          overlays = [
            cargo2nix.overlays.default
          ];
        };

        crossTargets = [
          rec { llvm = "x86_64-unknown-linux-musl"; rust = llvm; go = "amd64"; }
          rec { llvm = "aarch64-unknown-linux-musl"; rust = llvm; go = "arm64"; }
          { llvm = "armv6l-unknown-linux-musleabihf"; rust = "arm-unknown-linux-musleabihf"; go = "arm"; }
        ];


        ###
        #
        # ---- CROSS COMPILATION ---
        #
        ###
        cross = builtins.listToAttrs (map (triple: let
          # [i] isStatic is mandatory in our knowledge to get final rust binaries that do not depend on a linker
          # They also have a drawback we are aware of: it requires to recompile a C/C++ toolchain from scratch
          pkgsCross = import nixpkgs {
            system = system;
            overlays = [
              cargo2nix.overlays.default 
            ];
            crossSystem = {
              config = triple.llvm; # here we cross compile
              isStatic = true; # make sure resulting binary do not reference any linker
            };
          };

          # [i] All the patches to disable LTO that fails compilation
          k2vManifest = drv: drv.overrideCargoManifest + ''
# Rewrite Cargo.toml to disable LTO in release mode
# LTO breaks our build system with errors like "undefined reference to alloc::sync::Arc::<_>::drop_slow"
cat > Cargo.toml <<EOF
[workspace]
resolver = "2"
members = ["src/k2v-client/"]


[workspace.lints.clippy]
# pedantic lints configuration
doc_markdown = "warn"
format_collect = "warn"
manual_midpoint = "warn"
semicolon_if_nothing_returned = "warn"
unnecessary_semicolon = "warn"
unnecessary_wraps = "warn"
EOF
'';

          smtpMessageManifest = drv: drv.overrideCargoManifest + ''
# Rewrite Cargo.toml to disable LTO in release mode
# LTO breaks our build system with errors like "undefined reference to alloc::sync::Arc::<_>::drop_slow"
cat > Cargo.toml <<EOF
[workspace]
members = ["smtp-message/"]
EOF
'';


          smtpServerManifest = drv: drv.overrideCargoManifest + ''
# Rewrite Cargo.toml to disable LTO in release mode
# LTO breaks our build system with errors like "undefined reference to alloc::sync::Arc::<_>::drop_slow
cat > Cargo.toml <<EOF
[workspace]
members = ["smtp-server/"]
EOF
'';

          # [i] Cargo2nix build declaration
          project = pkgsCross.rustBuilder.makePackageSet({
            packageFun = import ./Cargo.nix;
            target = triple.rust;
            release = true;
            rustChannel = "nightly";
            packageOverrides = pkgs: pkgs.rustBuilder.overrides.all ++ [
              (pkgs.rustBuilder.rustLib.makeOverride {
                name = "smtp-message";
                overrideAttrs = drv: {
                    overrideCargoManifest = smtpMessageManifest drv;
                };
              })
              (pkgs.rustBuilder.rustLib.makeOverride {
                name = "smtp-server";
                overrideAttrs = drv: {
                    overrideCargoManifest = smtpServerManifest drv;
                };
              })
              (pkgs.rustBuilder.rustLib.makeOverride {
                name = "k2v-client";
                overrideAttrs = drv: {
                    overrideCargoManifest = k2vManifest drv;
                };
              })
              (pkgs.rustBuilder.rustLib.makeOverride {
                name = "aws-lc-sys";
                overrideAttrs = drv: {
                  nativeBuildInputs = drv.nativeBuildInputs or [] ++ (if triple.go == "arm" then [ 
                    # On armv6l, cmake and bindgen-cli are required.
                    pkgs.cmake
                    pkgs.rust-bindgen # fixup phase of pkgs.rustc on doc folder is slow (10+min) here. We might want to rewrite/skip it.
                  ] else [
                    # On aarch64 and amd64, it seems aws-lc-sys has vendored some code it needs
                    # We skip injecting these dependencies to make builds faster
                  ]);
                };
              })
            ];
          });
      
          # [i] Build final objects, all derivating from the cargo2nix decl
          crate = (project.workspace.aerogramme {});

          # [i] For static builds
	  bin = pkgs.stdenv.mkDerivation {
	    # volontarily break the dependency chain between crate
	    # and the rest of the nix ecosystem as we know our binary
	    # is statically compiled and thus the deps chain wrong.
            pname = "${crate.name}-bin";
            version = crate.version;
	    nativeBuildInputs = [ pkgs.removeReferencesTo ];
            dontUnpack = true;
            dontBuild = true;
            installPhase = ''
              cp ${crate.bin}/bin/aerogramme $out
	      remove-references-to -t ${project.rustToolchain} $out
            '';
          };

	  # [i] For container builds.
          # We add nix PKI root for HTTPS clients
	  fhs = pkgs.stdenv.mkDerivation {
            pname = "${crate.name}-fhs";
            version = crate.version;
	    nativeBuildInputs = [ pkgs.removeReferencesTo ];
            dontUnpack = true;
            dontBuild = true;
            installPhase = ''
              mkdir -p $out/bin
              cp ${crate.bin}/bin/aerogramme $out/bin/
	      remove-references-to -t ${project.rustToolchain} $out/bin/aerogramme
              cp -r ${pkgsCross.cacert}/etc $out/
            '';
          };

          container = pkgs.dockerTools.buildImage {
            name = "dxflrs/aerogramme";
            architecture = triple.go;
            copyToRoot = fhs;
            config = {
              Entrypoint = [ "/bin/aerogramme" ];
              Cmd = [ "--dev" "provider" "daemon" ];
            };
          };


          in
          { 
            # [i] Final "cross" object is built here:
            # only some of the created objects are exposed
            name = triple.go;
            value = rec {
              inherit crate bin container;
              default = bin;
            };
          }
        ) crossTargets);

        ###
        #
        # --- RELEASE TOOLING ---
        # expected to be call with nix run .#tools.build and nix run .#tools.push
        ###
        tools = rec {
          version = cross.amd64.crate.version; # bind version on amd64 crate
	  alba = albatros.packages.${system}.alba;
          build = pkgs.writeScriptBin "aerogramme-build" ''
#!/usr/bin/env bash
set -euxo pipefail

# static
nix build --print-build-logs .#cross.amd64.bin -o static/linux/amd64/aerogramme
nix build --print-build-logs .#cross.arm64.bin -o static/linux/arm64/aerogramme
nix build --print-build-logs .#cross.arm.bin   -o static/linux/arm/aerogramme

# containers
nix build --print-build-logs .#cross.amd64.container -o docker/linux.amd64.tar.gz
nix build --print-build-logs .#cross.arm64.container -o docker/linux.arm64.tar.gz
nix build --print-build-logs .#cross.arm.container   -o docker/linux.arm.tar.gz
        '';

        push = pkgs.writeScriptBin "aerogramme-publish" ''
#!/usr/bin/env bash
set -euxo pipefail

${alba} static push -t aerogramme:${version} static/ 's3://download.deuxfleurs.org?endpoint=garage.deuxfleurs.fr&s3ForcePathStyle=true&region=garage' 1>&2
${alba} container push -t aerogramme:${version} docker/ 's3://registry.deuxfleurs.org?endpoint=garage.deuxfleurs.fr&s3ForcePathStyle=true&region=garage' 1>&2
${alba} container push -t aerogramme:${version} docker/ "docker://docker.io/dxflrs/aerogramme:${version}" 1>&2
        '';


        };

	shell = pkgs.mkShell {
          buildInputs = [
            pkgs.openssl
            pkgs.pkg-config
	    pkgs.rust-bin.nightly.latest.default
	    cargo2nix.packages.${system}.cargo2nix
          ];
        };

      in
        {
          meta = {
            version = tools.version; 
          };

	  devShells.default = shell;

          packages = {
            inherit cross tools;
          };
        }
    );
}
