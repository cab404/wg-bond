{
  inputs.flake-compat = {
    url = "github:edolstra/flake-compat";
    flake = false;
  };
  outputs = { self, nixpkgs, ... }:
    let onPkgs = fn: builtins.mapAttrs fn nixpkgs.legacyPackages;
    in {
      defaultPackage = onPkgs (_: pkgs:
        pkgs.rustPlatform.buildRustPackage {
          name = "wg-bond";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        });

      devShell = onPkgs (_: pkgs:
        with pkgs;
        mkShell {
          buildInputs = [ pre-commit nixpkgs-fmt cargo rustc clippy rustfmt ];
          RUST_SRC_PATH=rustPlatform.rustLibSrc;
          shellHook = ''
            [ -e .git/hooks/pre-commit ] || pre-commit install --install-hooks
          '';
        });
    };

}
