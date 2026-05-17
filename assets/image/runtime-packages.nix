# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

let
  system = builtins.currentSystem;

  pkgs =
    (builtins.getFlake "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1")
    .legacyPackages.${system};
  pkgsMine = (builtins.getFlake "github:hnjae/nix-packages").packages.${system};
in
{
  runtime = pkgs.buildEnv {
    name = "container-runtime";
    paths = [
      # Devtools
      pkgs.gh
      pkgs.just

      # Shell
      pkgs.devenv
      pkgs.direnv
      pkgs.nix-direnv
      pkgs.fzf
      pkgs.ast-grep
      pkgs.yq-go

      # Custom
      pkgsMine."comment-checker" # required by oh-my-openagent
    ];
  };
}
