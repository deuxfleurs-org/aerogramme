{
  description = "Aerogramme";

  inputs = {
    cargo2nix = {
      type = "github";
      owner = "Alexis211";
      repo = "cargo2nix";
      ref = "custom_unstable";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    #cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.11.0";
    fenix.url = "github:nix-community/fenix/monthly";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, cargo2nix, flake-utils, fenix }: 
    flake-utils.lib.eachSystem [
      "x86_64-unknown-linux-musl"
      "aarch64-unknown-linux-musl"
      "armv6l-unknown-linux-musleabihf"
    ] (targetHost: let
    pkgs = import nixpkgs { 
      system = "x86_64-linux"; # hardcoded as we will cross compile
      crossSystem = {
        config = targetHost; # here we cross compile
        isStatic = true;
      };
      overlays = [cargo2nix.overlays.default];
    };

    shell = pkgs.mkShell {
      buildInputs = [
        cargo2nix.packages.x86_64-linux.default
      ];
    };
    
    rustPkgs = pkgs.rustBuilder.makePackageSet({
      rustToolchain = with fenix.packages.x86_64-linux; combine [
        minimal.cargo
        minimal.rustc
        targets.${targetHost}.latest.rust-std
      ];

      packageFun = import ./Cargo.nix;
    });

    in {
      devShells.default = shell;
      packages.aerogramme =  (rustPkgs.workspace.aerogramme {}).bin;
      packages.default = self.packages.${targetHost}.aerogramme;
    });
}
