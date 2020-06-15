let
  moz_overlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  pkgs = import <nixpkgs> { overlays = [ moz_overlay ]; };
  ruststable = (pkgs.latest.rustChannels.stable.rust.override {
    extensions = [ "rust-src" "rls-preview" "rust-analysis" "rustfmt-preview" "clippy-preview" ];
  });
in
with pkgs; mkShell {
  buildInputs = [
    rustup ruststable
    pre-commit
  ];

  shellHook = ''
    pre-commit install
  '';
}
