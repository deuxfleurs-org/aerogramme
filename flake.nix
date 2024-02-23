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

    # import alba releasing tool
    albatros.url = "git+https://git.deuxfleurs.fr/Deuxfleurs/albatros.git?ref=main";
  };

  outputs = { self, nixpkgs, cargo2nix, flake-utils, fenix, albatros }: 
    let platformArtifacts = flake-utils.lib.eachSystem [
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

    rustTarget = if targetHost == "armv6l-unknown-linux-musleabihf" then "arm-unknown-linux-musleabihf" else targetHost;

    # release builds
    rustRelease = pkgs.rustBuilder.makePackageSet({
      packageFun = import ./Cargo.nix;
      target = rustTarget;
      release = true;
      #rustcLinkFlags = [ "--cfg" "tokio_unstable" ];
      #rustcBuildFlags = [ "--cfg" "tokio_unstable" ];
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


    crate = (rustRelease.workspace.aerogramme {});

    # binary extract
    bin = pkgs.stdenv.mkDerivation {
      pname = "${crate.name}-bin";
      version = crate.version;
      dontUnpack = true;
      dontBuild = true;
      installPhase = ''
        cp ${crate.bin}/bin/aerogramme $out
      '';
    };

    # fhs extract
    fhs = pkgs.stdenv.mkDerivation {
      pname = "${crate.name}-fhs";
      version = crate.version;
      dontUnpack = true;
      dontBuild = true;
      installPhase = ''
      	mkdir -p $out/bin
        cp ${crate.bin}/bin/aerogramme $out/bin/
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
      copyToRoot = fhs;
      config = {
      	Env = [ "PATH=/bin" ];
        Cmd = [ "aerogramme" "--dev" "provider" "daemon" ];
      };
    };

    in {
      meta = {
        version = crate.version;
      };
      packages = {
        inherit fhs container;
        debug = (rustDebug.workspace.aerogramme {}).bin;
        aerogramme = bin;
        default = self.packages.${targetHost}.aerogramme;
      };
    });

    ###
    #
    # RELEASE STUFF
    #
    ###
    gpkgs = import nixpkgs {
      system = "x86_64-linux"; # hardcoded as we will cross compile
    };
    alba = albatros.packages.x86_64-linux.alba;

    # Shell
    shell = gpkgs.mkShell {
      buildInputs = [
        cargo2nix.packages.x86_64-linux.default
        fenix.packages.x86_64-linux.minimal.toolchain
        fenix.packages.x86_64-linux.rust-analyzer
      ];
      shellHook = ''
        echo "AEROGRAME DEVELOPMENT SHELL ${fenix.packages.x86_64-linux.minimal.rustc}"
        export RUST_SRC_PATH="${fenix.packages.x86_64-linux.latest.rust-src}/lib/rustlib/src/rust/library"
        export RUST_ANALYZER_INTERNALS_DO_NOT_USE='this is unstable'
      '';
    };

    # Used only to fetch the "version"
    version = platformArtifacts.meta.x86_64-unknown-linux-musl.version;

    build = gpkgs.writeScriptBin "aerogramme-build" ''
        set -euxo pipefail

        # static
        nix build --print-build-logs .#packages.x86_64-unknown-linux-musl.aerogramme  -o static/linux/amd64/aerogramme
        nix build --print-build-logs .#packages.aarch64-unknown-linux-musl.aerogramme -o static/linux/arm64/aerogramme
        nix build --print-build-logs .#packages.armv6l-unknown-linux-musleabihf.aerogramme  -o static/linux/arm/aerogramme

        # containers
        nix build --print-build-logs .#packages.x86_64-unknown-linux-musl.container  -o docker/linux.amd64.tar.gz
        nix build --print-build-logs .#packages.aarch64-unknown-linux-musl.container -o docker/linux.arm64.tar.gz
        nix build --print-build-logs .#packages.armv6l-unknown-linux-musleabihf.container  -o docker/linux.arm.tar.gz
        '';

    push = gpkgs.writeScriptBin "aerogramme-publish" ''
        set -euxo pipefail

        ${alba} static push -t aerogramme:${version} static/ 's3://download.deuxfleurs.org?endpoint=garage.deuxfleurs.fr&s3ForcePathStyle=true&region=garage' 1>&2
        ${alba} container push -t aerogramme:${version} docker/ 's3://registry.deuxfleurs.org?endpoint=garage.deuxfleurs.fr&s3ForcePathStyle=true&region=garage' 1>&2
        ${alba} container push -t aerogramme:${version} docker/ "docker://docker.io/dxflrs/aerogramme:$RTAG" 1>&2
        '';

    in
    {
        devShells.x86_64-linux.default = shell;
        packages = {
          x86_64-linux = {
            inherit build push;
          };
        } // platformArtifacts.packages;
    };
}
