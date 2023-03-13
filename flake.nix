{
  description = "Aerogramme";

  inputs = {
    cargo2nix = {
      type = "github";
      owner = "Alexis211";
      repo = "cargo2nix";
      ref = "custom_unstable";
    };
  };

  outputs = { self, nixpkgs, cargo2nix }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
    in {
    devShells.x86_64-linux.default = pkgs.mkShell {
      buildInputs = [
        cargo2nix.packages.x86_64-linux.default
      ];
    };
    packages.x86_64-linux.aerogramme = nixpkgs.legacyPackages.x86_64-linux.hello;
    packages.x86_64-linux.default = self.packages.x86_64-linux.aerogramme;
  };
}
