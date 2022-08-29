{ pkgs ? import <nixpkgs> {}, release ? true }:

with pkgs;
let cargo_nix = callPackage ./Cargo.nix {};
in cargo_nix.rootCrate.build
