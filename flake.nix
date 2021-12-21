{

  description = "Wireguard configuration manager";

  inputs = {
    fenix.url = "github:nix-community/fenix";
    nixpkgs.follows = "fenix/nixpkgs";

    utils.url = "github:numtide/flake-utils";

    flake-compat.url = "github:edolstra/flake-compat";
    flake-compat.flake = false;

  };

  outputs = args@{ self, nixpkgs, utils, fenix, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        fenixArch = fenix.packages.${system};

        rustChannel = fenixArch.latest;
        rustDevToolchain = rustChannel.withComponents [ "cargo" "clippy" "rust-src" "rust-std" "rustc" "rustfmt" ];
        rustToolchain = rustChannel.withComponents [ "cargo" "rust-std" "rustc" ];

        platformParams = {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        rustPlatform = pkgs.makeRustPlatform platformParams;

        # why not? maybe you want to run this on a mips router?
        getVersionFromTarget = target: target.latest.toolchain;
        fenixStaticPlatforms = let
          filteredPlatforms = with builtins; filter (s: match ".*-musl" s != null) (attrNames fenixArch.targets);
          kvPlatforms = map (k: nixpkgs.lib.nameValuePair k (getVersionFromTarget fenixArch.targets.${k})) (filteredPlatforms);
        in
          builtins.listToAttrs kvPlatforms;


        staticTargets = builtins.mapAttrs (platformName: toolchain:
          let
            rustToolchainMusl = toolchain;
            buildStatic = builtins.hasAttr system fenixStaticPlatforms;
            staticPlatformParams = {
              cargo = rustToolchain;
              rustc = rustToolchainMusl;
            };
            staticRustPlatform = pkgs.makeRustPlatform staticPlatformParams;
          in
            rustPlatformBuild staticRustPlatform
        ) fenixStaticPlatforms;


        rustPlatformBuild = platform: platform.buildRustPackage {
          name = "wg-bond";
          src = ./.;
          cargoLock = { lockFile = ./Cargo.lock; };
        };

      in rec {
        defaultPackage = rustPlatformBuild rustPlatform;

        checks = {
          # For nixpkgs compatibility
          rustPlatformCheck = rustPlatformBuild rustPlatform;
        };

        defaultApp = {
          type = "app";
          program = "${self.defaultPackage."${system}"}/bin/wg-bond";
        };

        packages = {
          wg-bond = self.defaultPackage."${system}";
        } // (staticTargets);

        devShell = with pkgs; mkShell {
          buildInputs = [ rustToolchain pre-commit ];
          shellHook = ''
            [ -e .git/hooks/pre-commit ] || pre-commit install --install-hooks
          '';
        };

    });
}
