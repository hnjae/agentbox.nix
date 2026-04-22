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
            postInstall = ''
              mkdir -p "$out/share/${projectName}"
              cp -R ${../../container-example} "$out/share/${projectName}/container-example"
              chmod -R u+w "$out/share/${projectName}/container-example"
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
