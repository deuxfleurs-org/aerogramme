{
  description = "Aerogramme";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    flake-utils.url = "github:numtide/flake-utils";

    # this patched version of cargo2nix makes easier to use clippy for building
    cargo2nix = {
      type = "github";
      owner = "Alexis211";
      repo = "cargo2nix";
      ref = "custom_unstable";
    };

    # use rust project builds 
    fenix.url = "github:nix-community/fenix/monthly";
  };

  outputs = { self, nixpkgs, cargo2nix, flake-utils, fenix }: 
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "x86_64-unknown-linux-musl"
      "aarch64-unknown-linux-musl"
      "armv6l-unknown-linux-musleabihf"
    ] (targetHost: let
    
    # with fenix, we get builds from the rust project.
    # they are done with an old version of musl (prior to 1.2.x that is used in NixOS),
    # however musl has a breaking change from 1.1.x to 1.2.x on 32 bit systems.
    # so we pin the lib to 1.1.x to avoid recompiling rust ourselves.
    muslOverlay = self: super: {
      musl = super.musl.overrideAttrs(old: if targetHost == "armv6l-unknown-linux-musleabihf" then rec {
        pname = "musl";
        version = "1.1.24";
        src = builtins.fetchurl {
          url    = "https://musl.libc.org/releases/${pname}-${version}.tar.gz";
          sha256 = "sha256:18r2a00k82hz0mqdvgm7crzc7305l36109c0j9yjmkxj2alcjw0k";
        };
      } else {});
    };

    pkgs = import nixpkgs { 
      system = "x86_64-linux"; # hardcoded as we will cross compile
      crossSystem = {
        config = targetHost; # here we cross compile
        isStatic = true;
      };
      overlays = [
        cargo2nix.overlays.default
        muslOverlay
      ];
    };

    pkgVanilla = import nixpkgs { system = "x86_64-linux"; };

    shell = pkgVanilla.mkShell {
      buildInputs = [
        cargo2nix.packages.x86_64-linux.default
        fenix.packages.x86_64-linux.minimal.toolchain
      ];
      shellHook = ''
        echo "AEROGRAME DEVELOPMENT SHELL ${fenix.packages.x86_64-linux.minimal.rustc}"
        export RUST_SRC_PATH="${fenix.packages.x86_64-linux.latest.rust-src}/lib/rustlib/src/rust/library"
      '';
    };

    rustTarget = if targetHost == "armv6l-unknown-linux-musleabihf" then "arm-unknown-linux-musleabihf" else targetHost;

    # release builds
    rustRelease = pkgs.rustBuilder.makePackageSet({
      packageFun = import ./Cargo.nix;
      target = rustTarget;
      release = true;
      rustToolchain = with fenix.packages.x86_64-linux; combine [
        minimal.cargo
        minimal.rustc
        targets.${rustTarget}.latest.rust-std
      ];
    });

    # debug builds with clippy as the compiler (hack to speed up compilation)
    debugBuildEnv = (drv:
    ''
        ${drv.setBuildEnv or ""}
        echo
        echo --- BUILDING WITH CLIPPY ---
        echo

        export NIX_RUST_BUILD_FLAGS="''${NIX_RUST_BUILD_FLAGS} --deny warnings"
        export NIX_RUST_LINK_FLAGS="''${NIX_RUST_LINK_FLAGS} --deny warnings"
        export RUSTC="''${CLIPPY_DRIVER}"
    '');

    rustDebug = pkgs.rustBuilder.makePackageSet({
      packageFun = import ./Cargo.nix;
      target = rustTarget;
      release = false;
      rustToolchain = with fenix.packages.x86_64-linux; combine [
        default.cargo
        default.rustc
        default.clippy
        targets.${rustTarget}.latest.rust-std
      ];
      packageOverrides = pkgs: pkgs.rustBuilder.overrides.all ++ [
        (pkgs.rustBuilder.rustLib.makeOverride {
          name = "aerogramme";
          overrideAttrs = drv: {
            setBuildEnv = (debugBuildEnv drv);
          };
        })
        (pkgs.rustBuilder.rustLib.makeOverride {
          name = "smtp-message";
          overrideAttrs = drv: {
            /*setBuildEnv = (traceBuildEnv drv);
            propagatedBuildInputs = drv.propagatedBuildInputs or [ ] ++ [
              traceRust    
            ];*/
          };
        })
      ];
    });

    # binary extract
    bin = pkgs.stdenv.mkDerivation {
      pname = "aerogramme-bin";
      version = "0.1.0";
      dontUnpack = true;
      dontBuild = true;
      installPhase = ''
        cp ${(rustRelease.workspace.aerogramme {}).bin}/bin/aerogramme $out
      '';
    };

    # docker packaging
    archMap = {
      "x86_64-unknown-linux-musl" = {
        GOARCH = "amd64";
      };
      "aarch64-unknown-linux-musl" = {
        GOARCH = "arm64";
      };
      "armv6l-unknown-linux-musleabihf" = {
        GOARCH = "arm";
      };
    };
    container = pkgs.dockerTools.buildImage {
      name = "dxflrs/aerogramme";
      architecture = (builtins.getAttr targetHost archMap).GOARCH;
      config = {
       Cmd = [ "${bin}" "server" ];
      };
    };

    in {
      devShells.default = shell;
      packages.debug = (rustDebug.workspace.aerogramme {}).bin;
      packages.aerogramme = bin;
      packages.container = container;
      packages.default = self.packages.${targetHost}.aerogramme;
    });
}
