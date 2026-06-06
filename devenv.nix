# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

{
  pkgs,
  lib,
  config,
  ...
}:
{
  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    channel = "nixpkgs";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
    ];
  };

  packages = with pkgs; [
    cargo-nextest

    # Configured in treefmt:
    just
    nixfmt
    rumdl
    shellcheck
    shellharden
    shfmt
    taplo
    yamlfmt
  ];

  treefmt.enable = true;
  treefmt.config =
    let
      shellFilePatterns = [
        "*.sh"
        "*.bash"
        ".envrc"
        ".envrc.*"
        ".env"
        ".env.*"
      ];
    in
    {
      projectRootFile = "flake.nix";
      settings = {
        excludes = [
          "*.lock"
        ];

        formatter.shellcheck.options = [ "-x" ];

        formatter.shellharden = {
          command = lib.getExe pkgs.shellharden;
          options = [ "--replace" ];
          includes = shellFilePatterns;
          priority = 10;
        };
      };

      programs = {
        biome.enable = true;
        just.enable = true;
        nixfmt.enable = true;
        rumdl-format.enable = true;
        rustfmt.enable = true;
        taplo.enable = true;
        yamlfmt.enable = true;

        # Shell
        shellcheck = {
          enable = true;
          includes = shellFilePatterns;
          priority = 20;
        };
        shfmt = {
          enable = true;
          includes = shellFilePatterns;
          priority = 30;
          useEditorConfig = true;
        };
      };
    };

  # https://devenv.sh/git-hooks/
  git-hooks.package = pkgs.prek;
  git-hooks.excludes = [ ".*\\.lock$" ];
  git-hooks.hooks = {
    # Static checkers
    detect-private-keys.enable = true;
    cocogitto = {
      enable = true;
      name = "cog verify";
      description = "Lint commit messages with Cocogitto.";
      package = pkgs.cocogitto;
      entry = "${lib.getExe pkgs.cocogitto} verify --file";
      stages = [ "commit-msg" ];
    };
    typos.enable = true;

    # Formatter check
    treefmt.enable = true;

    # Miscellaneous Checkers/Linters:
    deadnix.enable = true;
    statix.enable = true;
    rumdl.enable = true;
    reuse = {
      enable = true;
      name = "reuse lint";
      description = "Check REUSE license metadata.";
      package = pkgs.reuse;
      entry = "${lib.getExe pkgs.reuse} lint";
      always_run = true;
      pass_filenames = false;
    };
  };

  tasks = {
    "ci:test" = {
      exec = ''
        cargo nextest run
        cargo test --doc
      '';
    };

    "ci:lint" = {
      exec = "cargo clippy --all-targets --all-features -- --deny warnings";
      after = [
        "ci:test@succeeded"
      ];
    };

    "ci:format-check" = {
      exec = "cargo fmt --check";
      after = [
        "ci:lint@succeeded"
      ];
    };

    "ci:git-hooks" = {
      exec = "${lib.getExe config.git-hooks.package} run --all-files";
      after = [
        "ci:format-check@succeeded"
      ];
    };
  };
}
