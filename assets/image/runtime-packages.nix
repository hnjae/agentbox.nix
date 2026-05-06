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
      pkgs.git
      pkgs.git-filter-repo
      pkgs.git-lfs

      # Shell
      pkgs.devenv
      pkgs.direnv
      pkgs.nix-direnv
      pkgs.fzf
      pkgs.ripgrep
      pkgs.ast-grep

      # Custom
      pkgsMine."comment-checker" # required by oh-my-openagent
    ];
  };
}
