# SPDX-FileCopyrightText: 2026 KIM Hyunjae
# SPDX-License-Identifier: AGPL-3.0-or-later

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
      cargo = fromTOML (builtins.readFile ../../Cargo.toml);
      projectName = cargo.package.name;

      craneLib = inputs.crane.mkLib pkgs;
      src =
        let
          repoRoot = toString ../..;
          assetsRoot = "${repoRoot}/assets";
          imageRoot = "${assetsRoot}/image";
          completionRoot = "${repoRoot}/src/commands/completion";
          testFixturesRoot = "${repoRoot}/tests/fixtures";
        in
        pkgs.lib.cleanSourceWith {
          src = ../..;
          filter =
            path: type:
            let
              pathString = toString path;
              isEmbeddedImageAsset =
                pathString == assetsRoot
                || pathString == imageRoot
                || pkgs.lib.hasPrefix "${imageRoot}/" pathString;
              isTestFixture =
                pathString == testFixturesRoot || pkgs.lib.hasPrefix "${testFixturesRoot}/" pathString;
              isCompletionTemplate =
                pathString == completionRoot || pkgs.lib.hasPrefix "${completionRoot}/" pathString;
            in
            craneLib.filterCargoSources path type
            || isEmbeddedImageAsset
            || isTestFixture
            || isCompletionTemplate;
        };

      commonArgs = {
        inherit src;
        strictDeps = true;
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in
    {
      config = {
        packages.${projectName} = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
            cargoBuildExtraArgs = "--bin ${projectName}";
            nativeCheckInputs = [
              pkgs.git
            ];
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
              "$out/bin/${projectName}" __generate-manpages \
                "$out/share/man/man1"
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
