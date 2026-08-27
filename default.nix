{ pkgs ? import <nixpkgs> { } }:
pkgs.callPackage ./packaging/nix/on-air-core.nix {
  src = pkgs.lib.cleanSource ./.;
}
