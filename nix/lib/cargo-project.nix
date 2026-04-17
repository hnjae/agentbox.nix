{ crane, pkgs }:
rec {
  cargo = fromTOML (builtins.readFile ../../Cargo.toml);

  projectName = cargo.package.name;

  craneLib = crane.mkLib pkgs;
  src = craneLib.cleanCargoSource ../..;

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
