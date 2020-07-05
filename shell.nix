let
  pin = import ./nix/sources.nix;
  moz_overlay = import pin.nixpkgs-mozilla;
  pkgs = import pin.nixpkgs { overlays = [ moz_overlay ]; };
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
