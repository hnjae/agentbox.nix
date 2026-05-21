// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::Result;

use super::environment::DevEnvironment;
use super::nix::{HostNixDevShellProbe, NixDevShellProbe, resolve_nix_develop};
use super::path::{nearest_file, parent_directory};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DevEnvironmentDiscovery {
    envrc: Option<Utf8PathBuf>,
    devenv_nix: Option<Utf8PathBuf>,
    flake_nix: Option<Utf8PathBuf>,
}

impl DevEnvironmentDiscovery {
    pub(super) fn detect(target_directory: &Utf8Path, git_root: &Utf8Path) -> Self {
        Self {
            envrc: nearest_file(target_directory, git_root, ".envrc"),
            devenv_nix: nearest_file(target_directory, git_root, "devenv.nix"),
            flake_nix: nearest_file(target_directory, git_root, "flake.nix"),
        }
    }

    pub(super) fn resolve(self, target_directory: &Utf8Path) -> Result<DevEnvironment> {
        self.resolve_with_probe(target_directory, &HostNixDevShellProbe)
    }

    fn resolve_with_probe(
        self,
        target_directory: &Utf8Path,
        probe: &impl NixDevShellProbe,
    ) -> Result<DevEnvironment> {
        if self.envrc.is_some() {
            return Ok(DevEnvironment::Direnv);
        }

        if let Some(devenv_nix) = self.devenv_nix {
            return Ok(DevEnvironment::Devenv {
                root: parent_directory(&devenv_nix, "devenv.nix"),
            });
        }

        let Some(flake_nix) = self.flake_nix else {
            return Ok(DevEnvironment::None);
        };
        let flake_root = parent_directory(&flake_nix, "flake.nix");

        resolve_nix_develop(target_directory, &flake_root, probe)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;

    use crate::Error;

    use super::*;

    #[test]
    fn discovery_ignores_envrc_above_git_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let repo = root.join("repo");
        let nested = repo.join("nested");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&nested).unwrap();
        fs::write(root.join(".envrc"), "use nix\n").unwrap();

        let discovery = DevEnvironmentDiscovery::detect(&nested, &repo);

        assert_eq!(
            discovery,
            DevEnvironmentDiscovery {
                envrc: None,
                devenv_nix: None,
                flake_nix: None,
            }
        );
    }

    #[test]
    fn discovery_records_nearest_supported_files_within_git_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let repo = Utf8PathBuf::from_path_buf(sandbox.path().join("repo")).unwrap();
        let nested = repo.join("nested");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&nested).unwrap();
        fs::write(repo.join(".envrc"), "use nix\n").unwrap();
        fs::write(repo.join("flake.nix"), "{}\n").unwrap();
        fs::write(nested.join("devenv.nix"), "{}\n").unwrap();

        let discovery = DevEnvironmentDiscovery::detect(&nested, &repo);

        assert_eq!(
            discovery,
            DevEnvironmentDiscovery {
                envrc: Some(repo.join(".envrc")),
                devenv_nix: Some(nested.join("devenv.nix")),
                flake_nix: Some(repo.join("flake.nix")),
            }
        );
    }

    #[test]
    fn envrc_takes_precedence_over_other_development_environment_files() {
        let discovery = DevEnvironmentDiscovery {
            envrc: Some("/repo/.envrc".into()),
            devenv_nix: Some("/repo/devenv.nix".into()),
            flake_nix: Some("/repo/flake.nix".into()),
        };

        assert_eq!(
            discovery.resolve(Utf8Path::new("/repo")).unwrap(),
            DevEnvironment::Direnv
        );
    }

    #[test]
    fn devenv_takes_precedence_over_flake_when_envrc_is_absent() {
        let discovery = DevEnvironmentDiscovery {
            envrc: None,
            devenv_nix: Some("/repo/nested/devenv.nix".into()),
            flake_nix: Some("/repo/flake.nix".into()),
        };

        assert_eq!(
            discovery.resolve(Utf8Path::new("/repo/nested")).unwrap(),
            DevEnvironment::Devenv {
                root: "/repo/nested".into()
            }
        );
    }

    #[test]
    fn flake_resolution_uses_first_existing_candidate_from_probe() {
        let discovery = DevEnvironmentDiscovery {
            envrc: None,
            devenv_nix: None,
            flake_nix: Some("/repo/flake.nix".into()),
        };
        let probe = RecordingProbe::new(["default"]);

        let environment = discovery
            .resolve_with_probe(Utf8Path::new("/repo/api"), &probe)
            .unwrap();

        assert_eq!(
            environment,
            DevEnvironment::NixDevelop {
                flake_root: "/repo".into(),
                attr: "default".to_string(),
            }
        );
        assert_eq!(
            probe.calls(),
            vec![
                ("/repo".to_string(), "api".to_string()),
                ("/repo".to_string(), "default".to_string()),
            ]
        );
    }

    #[test]
    fn flake_resolution_stops_after_first_existing_candidate() {
        let discovery = DevEnvironmentDiscovery {
            envrc: None,
            devenv_nix: None,
            flake_nix: Some("/repo/flake.nix".into()),
        };
        let probe = RecordingProbe::new(["api"]);

        let environment = discovery
            .resolve_with_probe(Utf8Path::new("/repo/api"), &probe)
            .unwrap();

        assert_eq!(
            environment,
            DevEnvironment::NixDevelop {
                flake_root: "/repo".into(),
                attr: "api".to_string(),
            }
        );
        assert_eq!(
            probe.calls(),
            vec![("/repo".to_string(), "api".to_string())]
        );
    }

    #[test]
    fn flake_resolution_returns_none_when_no_candidate_shell_exists() {
        let discovery = DevEnvironmentDiscovery {
            envrc: None,
            devenv_nix: None,
            flake_nix: Some("/repo/flake.nix".into()),
        };
        let probe = RecordingProbe::new([]);

        let environment = discovery
            .resolve_with_probe(Utf8Path::new("/repo/api"), &probe)
            .unwrap();

        assert_eq!(environment, DevEnvironment::None);
        assert_eq!(
            probe.calls(),
            vec![
                ("/repo".to_string(), "api".to_string()),
                ("/repo".to_string(), "default".to_string()),
            ]
        );
    }

    #[test]
    fn flake_resolution_propagates_probe_errors() {
        let discovery = DevEnvironmentDiscovery {
            envrc: None,
            devenv_nix: None,
            flake_nix: Some("/repo/flake.nix".into()),
        };
        let probe = FailingProbe;

        let error = discovery
            .resolve_with_probe(Utf8Path::new("/repo/api"), &probe)
            .unwrap_err();

        assert_eq!(error.to_string(), "probe failed");
    }

    struct RecordingProbe {
        existing_attrs: Vec<String>,
        calls: RefCell<Vec<(String, String)>>,
    }

    impl RecordingProbe {
        fn new<const N: usize>(existing_attrs: [&str; N]) -> Self {
            Self {
                existing_attrs: existing_attrs.into_iter().map(str::to_string).collect(),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, String)> {
            self.calls.borrow().clone()
        }
    }

    impl NixDevShellProbe for RecordingProbe {
        fn dev_shell_exists(&self, flake_root: &Utf8Path, attr: &str) -> Result<bool> {
            self.calls
                .borrow_mut()
                .push((flake_root.to_string(), attr.to_string()));
            Ok(self.existing_attrs.iter().any(|existing| existing == attr))
        }
    }

    struct FailingProbe;

    impl NixDevShellProbe for FailingProbe {
        fn dev_shell_exists(&self, _flake_root: &Utf8Path, _attr: &str) -> Result<bool> {
            Err(Error::msg("probe failed"))
        }
    }
}
