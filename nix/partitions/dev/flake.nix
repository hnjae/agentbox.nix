# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

{
  description = "Development-only inputs for the dev partition";

  inputs = {
    dev-nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1";
    git-hooks = {
      url = "https://flakehub.com/f/cachix/git-hooks.nix/0.1.1205";
      inputs.nixpkgs.follows = "dev-nixpkgs";
      inputs.flake-compat.follows = "";
    };
  };

  outputs = inputs: {
    inherit inputs;
  };
}
