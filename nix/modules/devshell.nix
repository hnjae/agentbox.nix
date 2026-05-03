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
                detect-private-keys.enable = true;
                editorconfig-checker.enable = true;
                gitlint.enable = true;
                typos.enable = true;

                # Formatters:
                nixfmt.enable = true;
                rustfmt.enable = true;

                # Other formatters:
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
              };
            };
          };

          devShells.default = craneLib.devShell {
            inherit (config.pre-commit) shellHook;

            checks = self.checks.${system};

            packages = lib.flatten [
              config.pre-commit.settings.enabledPackages

              (with devPkgs; [
                # Nix
                nixd
                statix
                deadnix

                # Shell
                shellcheck
                shellharden

                # Misc
                editorconfig-checker
              ])
            ];
          };
        };
    };
}
