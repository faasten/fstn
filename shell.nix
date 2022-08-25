let 
  base-nixpkgs = import <nixpkgs> {};
  mozillaOverlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  pkgs = import <nixpkgs> { overlays = [ mozillaOverlay ]; };
  rust = (pkgs.rustChannelOf { channel = "stable"; }).rust.override {
    targets = [ "x86_64-unknown-linux-musl" ];
  };
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rust;
    rustc = rust;
  };
in pkgs.mkShell {
  nativeBuildInputs = [
    rustPlatform.rust.cargo
    rustPlatform.rust.rustc
  ];
}
