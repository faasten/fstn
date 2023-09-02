{ pkgs ? import <nixpkgs> {}} :

pkgs.rustPlatform.buildRustPackage rec {
  pname = "fstn";
  version = "0.4.0";

  buildType = "release";

  src = builtins.filterSource
    (path: type: !(type == "directory" && baseNameOf path == "target"))
    ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  meta = {
    description = "A user-centric function-as-a-service platform";
    homepage = "https://github.com/faasten/fstn";
  };
}
