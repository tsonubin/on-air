# The repository as a build source, minus build outputs, dependency caches and
# VCS metadata. Shared by flake.nix and default.nix so both build the same tree.
{ lib, src }:
lib.cleanSourceWith {
  inherit src;
  filter =
    path: type:
    let
      base = baseNameOf path;
    in
    !builtins.elem base [
      "target"
      "node_modules"
      ".git"
      ".turbo"
      "dist"
      "test-results"
      "result"
      # Git worktrees checked out inside the repo: whole repo copies.
      "worktrees"
      ".worktrees"
    ];
}
