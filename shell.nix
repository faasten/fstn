let 
  pkgs = import <nixpkgs> {};
in pkgs.mkShell {
  nativeBuildInputs = [
    pkgs.lld
    pkgs.rustup
  ];
}
