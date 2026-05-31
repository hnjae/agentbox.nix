# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

{
  inputs = {
    nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0"; # stable
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    inputs@{
      flake-parts,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.flake-parts.flakeModules.partitions

        ./nix/modules/checks.nix
        ./nix/modules/devshell.nix
        ./nix/modules/package.nix
      ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      partitionedAttrs = {
        checks = "dev";
        devShells = "dev";
        formatter = "dev";
      };

      partitions.dev.extraInputsFlake = ./nix/partitions/dev;
      partitions.dev.module =
        { inputs, ... }:
        {
          imports = [
            inputs.git-hooks.flakeModule
          ];
        };
    };
}
