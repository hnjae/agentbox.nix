// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::runtime::RuntimeKind;
use crate::runtime::default_image::default_image_context_hash;

use super::image_state::RuntimeImageState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RuntimeImageUpdatePlan {
    RefreshState {
        state: RuntimeImageState,
        latest_version: String,
    },
    Rebuild {
        version: String,
    },
}

pub(super) fn plan_runtime_image_update(
    runtime: RuntimeKind,
    latest_version: String,
    image_exists: bool,
    prior_state: Option<RuntimeImageState>,
) -> RuntimeImageUpdatePlan {
    let image = runtime.default_image();
    let context_hash = default_image_context_hash();

    if let Some(state) = prior_state.filter(|state| {
        image_exists
            && state.installed_version == latest_version
            && state.image == image
            && state.image_context_hash == context_hash
    }) {
        RuntimeImageUpdatePlan::RefreshState {
            state,
            latest_version,
        }
    } else {
        RuntimeImageUpdatePlan::Rebuild {
            version: latest_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_plan_refreshes_state_when_image_state_and_version_are_current() {
        let runtime = RuntimeKind::Codex;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9);

        let plan = plan_runtime_image_update(runtime, "1.2.3".to_string(), true, Some(state));

        assert!(matches!(
            plan,
            RuntimeImageUpdatePlan::RefreshState {
                latest_version,
                ..
            } if latest_version == "1.2.3"
        ));
    }

    #[test]
    fn update_plan_rebuilds_when_image_is_missing_even_if_state_is_current() {
        let runtime = RuntimeKind::Codex;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9);

        let plan = plan_runtime_image_update(runtime, "1.2.3".to_string(), false, Some(state));

        assert_eq!(
            plan,
            RuntimeImageUpdatePlan::Rebuild {
                version: "1.2.3".to_string(),
            }
        );
    }

    #[test]
    fn update_plan_rebuilds_when_state_version_is_stale() {
        let runtime = RuntimeKind::Opencode;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9);

        let plan = plan_runtime_image_update(runtime, "1.2.4".to_string(), true, Some(state));

        assert_eq!(
            plan,
            RuntimeImageUpdatePlan::Rebuild {
                version: "1.2.4".to_string(),
            }
        );
    }
}
