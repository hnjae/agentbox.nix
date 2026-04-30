let
  system = builtins.currentSystem;

  pkgsRolling =
    (builtins.getFlake "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1")
    .legacyPackages.${system};
  pkgsStable =
    (builtins.getFlake "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0")
    .legacyPackages.${system};
  pkgsMine = (builtins.getFlake "github:hnjae/nix-packages").packages.${system};
in
{
  runtime = pkgsRolling.buildEnv {
    name = "container-runtime";
    paths = [
      pkgsRolling.opencode
      pkgsRolling.codex

      # Devtools
      pkgsRolling.gh
      pkgsRolling.just
      pkgsStable.git
      pkgsStable.git-filter-repo
      pkgsStable.git-lfs

      # Shell
      pkgsRolling.devenv
      pkgsStable.direnv
      pkgsStable.nix-direnv

      # Runtime
      pkgsStable.nodejs-slim

      # "Modern" utilities
      pkgsStable.bat
      pkgsStable.eza
      pkgsStable.fd
      pkgsStable.fzf
      pkgsStable.ripgrep
      pkgsStable.ast-grep

      # Custom
      pkgsMine."comment-checker" # required by oh-my-openagent
    ];
  };
}
