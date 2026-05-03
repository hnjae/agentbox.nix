{
  partitions.dev.module =
    { inputs, ... }:
    {
      perSystem =
        { pkgs, ... }:
        let
          project = import ../lib/cargo-project.nix {
            inherit pkgs;
            inherit (inputs) crane;
          };

          inherit (project)
            craneLib
            commonArgs
            projectName
            cargoArtifacts
            ;
        in
        {
          checks = {
            "${projectName}-lint" = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- --deny warnings";
              }
            );

            "${projectName}-test" = craneLib.cargoTest (
              commonArgs
              // {
                inherit cargoArtifacts;
                nativeCheckInputs = [
                  pkgs.git
                ];
              }
            );
          };
        };
    };
}
