{ pkgs ? import <nixpkgs> { } }:
pkgs.callPackage ./packaging/nix/on-air-core.nix {
  src = import ./packaging/nix/source.nix {
    inherit (pkgs) lib;
    src = ./.;
  };
}
