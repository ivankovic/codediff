# Lets `nix run github:ivankovic/codediff` and `nix build` work against this repository directly,
# with no tag, no release artifact and no vendor hash - `packaging/nix/package.nix` vendors straight
# from the committed Cargo.lock. See that file for the derivation itself and for what a nixpkgs
# submission would change.
{
  description = "Fast, robust, syntax-aware code diffing using tree-sitter ASTs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        # Read from Cargo.toml rather than repeated here, so a release bump touches one file.
        version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        codediff = pkgs.callPackage ./packaging/nix/package.nix {
          src = self;
          inherit version;
        };
      in
      {
        packages = {
          inherit codediff;
          default = codediff;
        };

        apps.default = {
          type = "app";
          program = "${codediff}/bin/codediff";
        };

        # `nix develop` for working on codediff itself: the full toolchain plus the tools the
        # Makefile's own targets reach for. Not needed to merely build or run the package.
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            cargo-nextest
            cargo-llvm-cov
            python3
            ruff
          ];
        };
      }
    );
}
