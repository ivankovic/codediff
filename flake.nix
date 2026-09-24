# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

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
