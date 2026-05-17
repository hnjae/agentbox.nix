# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

{
  partitions.dev.module =
    { inputs, self, ... }:
    {
      perSystem =
        {
          config,
          pkgs,
          system,
          lib,
          ...
        }:
        let
          project = import ../lib/cargo-project.nix {
            inherit pkgs;
            inherit (inputs) crane;
          };

          inherit (project)
            craneLib
            ;

          devPkgs = import inputs.dev-nixpkgs {
            inherit system;
            config.allowUnfree = true;
          };

          shellFormat = pkgs.writeShellApplication {
            name = "shell-format";
            runtimeInputs = with devPkgs; [
              shellharden
              shfmt
            ];
            text = ''
              for file in "$@"; do
                shellharden --replace "$file"
                shfmt --indent 4 --simplify --write "$file"
              done
            '';
          };
        in
        {
          formatter =
            let
              inherit (config.pre-commit.settings) package configFile;
            in
            pkgs.writeScriptBin "pre-commit-run" ''
              #!${pkgs.dash}/bin/dash

              exec ${lib.getExe package} run --hook-stage pre-commit --all-files --config ${configFile}
            '';

          pre-commit = {
            check.enable = true;
            pkgs = devPkgs;
            settings = {
              package = devPkgs.prek;
              default_stages = [ "pre-commit" ];
              gitPackage = pkgs.gitMinimal;
              hooks = {
                # Static checkers
                cocogitto = {
                  enable = true;
                  name = "cog verify";
                  description = "Require Conventional Commits with Cocogitto.";
                  package = devPkgs.cocogitto;
                  entry = "${lib.getExe devPkgs.cocogitto} verify --file";
                  stages = [ "commit-msg" ];
                };
                detect-private-keys.enable = true;
                editorconfig-checker.enable = true;
                typos.enable = true;

                # Nix Static Checkers:
                deadnix.enable = true;
                statix.enable = true;

                # Miscellaneous Static Checkers:
                reuse = {
                  enable = true;
                  name = "reuse lint";
                  description = "Check REUSE license metadata.";
                  package = devPkgs.reuse;
                  entry = "${lib.getExe devPkgs.reuse} lint";
                  always_run = true;
                  pass_filenames = false;
                };
                shellcheck-env = {
                  enable = true;
                  name = "shellcheck";
                  package = devPkgs.shellcheck;
                  files = ''
                    (?x)^(
                      .*\.(sh|bash)$|
                      \.envrc(\..+)?$|
                      \.env(\..+)?$
                    )
                  '';
                  entry = "${lib.getExe devPkgs.shellcheck} -e SC2034,SC1091,SC2154";
                };

                # Formatters:
                nixfmt.enable = true;
                rustfmt.enable = true;
                taplo.enable = true;
                rumdl.enable = true;
                just-format = {
                  enable = true;
                  name = "just-fmt";
                  files = ''(^|/)(\.)?[jJ]ustfile$'';
                  entry = toString (
                    pkgs.writeScript "pre-commit-just-fmt"
                      # sh
                      ''
                        #!${pkgs.dash}/bin/dash

                        set -eu

                        for file in "$@"; do
                          ${lib.getExe devPkgs.just} --unstable --fmt --justfile "$file"
                        done
                      ''
                  );
                };
                shell-format = {
                  enable = true;
                  name = "shell-format";
                  description = "Format shell files with shellharden and shfmt.";
                  package = shellFormat;
                  files = ''
                    (?x)^(
                      .*\.(sh|bash)$|
                      \.envrc(\..+)?$|
                      \.env(\..+)?$
                    )
                  '';
                  entry = lib.getExe shellFormat;
                };
              };
            };
          };

          devShells.default = craneLib.devShell {
            inherit (config.pre-commit) shellHook;

            checks = self.checks.${system};

            packages = lib.flatten [
              config.pre-commit.settings.enabledPackages

              (with devPkgs; [
                nixd
                shellharden
                shfmt
              ])
            ];
          };
        };
    };
}
