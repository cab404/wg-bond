{

  description = "Wireguard configuration manager";

  inputs = {
    naersk.url = "github:nmattia/naersk";
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
        compat = attr: ''
          (import (with import ${nixpkgs} {}; fetchFromGitHub {
            owner = "edolstra";
            repo = "flake-compat";
            rev = "${args.flake-compat.rev}";
            hash = "${args.flake-compat.narHash}";
          }) { src = ./.; })."${attr}"
        '';

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
          buildInputs = [ rustToolchain pre-commit
            (writeScriptBin "reinstall_compat" ''
              cat > default.nix << a-meaning-of-life
              ${compat "defaultNix"}a-meaning-of-life

              cat > shell.nix << mowmowimmacow
              ${compat "shellNix"}mowmowimmacow

              git add default.nix shell.nix
              git commit default.nix shell.nix -m "env: bumped flake-compat"
            '')
          ];
          shellHook = ''
            [ -e .git/hooks/pre-commit ] || pre-commit install --install-hooks
          '';
        };

    });
}
