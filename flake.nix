{

  description = "Wireguard configuration manager";

  inputs = {
    naersk.url = "github:nix-community/naersk";
    fenix.url = "github:nix-community/fenix";

    naersk.inputs.nixpkgs.follows = "fenix/nixpkgs";
    nixpkgs.follows = "fenix/nixpkgs";

    utils.url = "github:numtide/flake-utils";

    flake-compat.url = "github:edolstra/flake-compat";
    flake-compat.flake = false;

  };

  outputs = args@{ self, nixpkgs, utils, fenix, naersk, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        fenixArch = fenix.packages.${system};

        rustChannel = fenixArch.stable;
        rustToolchain = rustChannel.withComponents [ "cargo" "clippy" "rust-src" "rust-std" "rustc" "rustfmt" ];
        naersk-lib = naersk.lib.${system}.override {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

      in rec {
        inherit rustToolchain;
        defaultPackage = naersk-lib.buildPackage ./.;

        defaultApp = {
          type = "app";
          program = "${self.defaultPackage."${system}"}/bin/wg-bond";
        };

        devShell = with pkgs; mkShell {
          buildInputs = [ rustToolchain pre-commit ];
          shellHook = ''
            [ -e .git/hooks/pre-commit ] || pre-commit install --install-hooks
          '';
        };

    });
}
