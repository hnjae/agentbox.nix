{ inputs, ... }:
{
  perSystem =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      project = import ../lib/cargo-project.nix {
        inherit pkgs;
        inherit (inputs) crane;
      };

      inherit (project)
        cargo
        craneLib
        commonArgs
        cargoArtifacts
        projectName
        ;
    in
    {
      config = {
        packages.${projectName} = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            cargoBuildExtraArgs = "--bin ${projectName}";
            postInstall = ''
              mkdir -p \
                "$out/share/bash-completion/completions" \
                "$out/share/zsh/site-functions" \
                "$out/share/fish/vendor_completions.d" \
                "$out/share/man/man1"

              "$out/bin/${projectName}" __generate-completion bash \
                > "$out/share/bash-completion/completions/${projectName}"
              "$out/bin/${projectName}" __generate-completion zsh \
                > "$out/share/zsh/site-functions/_${projectName}"
              "$out/bin/${projectName}" __generate-completion fish \
                > "$out/share/fish/vendor_completions.d/${projectName}.fish"
              "$out/bin/${projectName}" __generate-man \
                > "$out/share/man/man1/${projectName}.1"
            '';
            meta = {
              mainProgram = projectName;
              description = cargo.package.description;
              license = lib.licenses.agpl3Plus;
              platforms = lib.platforms.linux;
            };
          }
        );

        packages.default = config.packages.${projectName};

        apps.default = {
          type = "app";
          program = lib.getExe config.packages.default;
        };
      };
    };
}
