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

    shell = pkgs.mkShell {
      buildInputs = [
        cargo2nix.packages.x86_64-linux.default
      ];
    };

    rustTarget = if targetHost == "armv6l-unknown-linux-musleabihf" then "arm-unknown-linux-musleabihf" else targetHost;
    
    rustPkgs = pkgs.rustBuilder.makePackageSet({
      packageFun = import ./Cargo.nix;
      target = rustTarget;
      rustToolchain = with fenix.packages.x86_64-linux; combine [
        minimal.cargo
        minimal.rustc
        targets.${rustTarget}.latest.rust-std
      ];
    });

    in {
      devShells.default = shell;
      packages.aerogramme =  (rustPkgs.workspace.aerogramme {}).bin;
      packages.default = self.packages.${targetHost}.aerogramme;
    });
}
