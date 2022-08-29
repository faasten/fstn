{ pkgs ? import <nixpkgs> {}, release ? true }:

with pkgs;
let cargo_nix = import ./Cargo.nix { inherit pkgs; };
in cargo_nix.rootCrate.build
