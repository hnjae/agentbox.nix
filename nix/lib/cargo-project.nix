{ crane, pkgs }:
rec {
  cargo = fromTOML (builtins.readFile ../../Cargo.toml);

  projectName = cargo.package.name;

  craneLib = crane.mkLib pkgs;
  src =
    let
      repoRoot = toString ../..;
      assetsRoot = "${repoRoot}/assets";
      imageRoot = "${assetsRoot}/image";
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
        in
        craneLib.filterCargoSources path type || isEmbeddedImageAsset || isTestFixture;
    };

  # Common arguments can be set here to avoid repeating them later
  commonArgs = {
    inherit src;
    strictDeps = true;

    buildInputs = [
      # Add additional build inputs here
    ];

    # Additional environment variables can be set directly
    # MY_CUSTOM_VAR = "some value";
  };

  # Build *just* the cargo dependencies, so we can reuse
  # all of that work (e.g. via cachix) when running in CI
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
}
