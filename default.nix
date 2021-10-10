(import (with import /nix/store/a6gaflr2x5if3xkidxpc3qd125ih0v3l-source {}; fetchFromGitHub {
  owner = "edolstra";
  repo = "flake-compat";
  rev = "12c64ca55c1014cdc1b16ed5a804aa8576601ff2";
  hash = "sha256-hY8g6H2KFL8ownSiFeMOjwPC8P0ueXpCVEbxgda3pko=";
}) { src = ./.; })."defaultNix"
