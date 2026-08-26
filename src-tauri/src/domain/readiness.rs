use crate::domain::bios::{BiosRequirementStatusState, SystemBiosStatus};
use crate::domain::core::{CoreId, CorePolicyDecision};
use crate::domain::runtime::RuntimeState;
use crate::domain::system::SystemDefinition;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReadinessReason {
    CorePolicyUnresolved {
        #[serde(rename = "researchItem")]
        research_item: String,
    },
    RuntimeUnavailable {
        state: RuntimeState,
    },
    MissingCore {
        #[serde(rename = "coreId")]
        core_id: CoreId,
    },
    MissingRequiredBios {
        #[serde(rename = "requirementId")]
        requirement_id: crate::domain::bios::BiosRequirementId,
    },
    InvalidRequiredBios {
        #[serde(rename = "requirementId")]
        requirement_id: crate::domain::bios::BiosRequirementId,
    },
    BiosIdentityNotCovered {
        #[serde(rename = "requirementId")]
        requirement_id: crate::domain::bios::BiosRequirementId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemReadiness {
    pub ready: bool,
    pub reasons: Vec<ReadinessReason>,
}

impl SystemReadiness {
    pub fn evaluate(
        system: &SystemDefinition,
        runtime_state: RuntimeState,
        available_core_ids: &BTreeSet<CoreId>,
        bios: &SystemBiosStatus,
    ) -> Self {
        let mut reasons = Vec::new();
        let runtime_available = matches!(
            runtime_state,
            RuntimeState::Ready | RuntimeState::RollbackAvailable
        );

        match &system.core_policy.decision {
            CorePolicyDecision::Resolved => {
                if !runtime_available {
                    reasons.push(ReadinessReason::RuntimeUnavailable {
                        state: runtime_state,
                    });
                } else if let Some(default_core_id) = &system.core_policy.default_core_id {
                    if !available_core_ids.contains(default_core_id) {
                        reasons.push(ReadinessReason::MissingCore {
                            core_id: default_core_id.clone(),
                        });
                    }
                }
            }
            CorePolicyDecision::Unresolved { research_item } => {
                reasons.push(ReadinessReason::CorePolicyUnresolved {
                    research_item: research_item.clone(),
                });
                if !runtime_available {
                    reasons.push(ReadinessReason::RuntimeUnavailable {
                        state: runtime_state,
                    });
                }
            }
        }

        for requirement in &bios.requirements {
            if !requirement.required {
                continue;
            }
            match requirement.state {
                BiosRequirementStatusState::PresentValid => {}
                BiosRequirementStatusState::Missing => {
                    reasons.push(ReadinessReason::MissingRequiredBios {
                        requirement_id: requirement.requirement_id.clone(),
                    });
                }
                BiosRequirementStatusState::PresentInvalid => {
                    reasons.push(ReadinessReason::InvalidRequiredBios {
                        requirement_id: requirement.requirement_id.clone(),
                    });
                }
                BiosRequirementStatusState::NotCoveredByCatalog
                | BiosRequirementStatusState::OptionalMissing => {
                    reasons.push(ReadinessReason::BiosIdentityNotCovered {
                        requirement_id: requirement.requirement_id.clone(),
                    });
                }
            }
        }

        Self {
            ready: reasons.is_empty(),
            reasons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReadinessReason, SystemReadiness};
    use crate::domain::bios::{
        BiosPolicy, BiosRequirementId, BiosRequirementStatus, BiosRequirementStatusState,
        SystemBiosStatus,
    };
    use crate::domain::core::{CoreId, CorePolicy};
    use crate::domain::runtime::RuntimeState;
    use crate::domain::system::{SystemDefinition, SystemId};
    use std::collections::BTreeSet;

    fn resolved_system() -> (SystemDefinition, CoreId) {
        let core_id = CoreId::new("synthetic-core").unwrap();
        (
            SystemDefinition {
                id: SystemId::Nes,
                display_name: "Nintendo Entertainment System".to_owned(),
                manufacturer: "Nintendo".to_owned(),
                aliases: vec!["NES".to_owned()],
                supported_extensions: vec![".nes".to_owned()],
                core_policy: CorePolicy::resolved(core_id.clone(), vec![core_id.clone()]),
                bios_policy: BiosPolicy::NotRequired,
                bios_requirements: Vec::new(),
            },
            core_id,
        )
    }

    fn bios_status(state: BiosRequirementStatusState, required: bool) -> SystemBiosStatus {
        let requirement = BiosRequirementStatus {
            requirement_id: BiosRequirementId::new("synthetic-bios").unwrap(),
            system_id: SystemId::Nes,
            required,
            state,
            expected_filenames: vec!["firmware.bin".to_owned()],
            expected_size_bytes: None,
            description: "Synthetic BIOS requirement".to_owned(),
            matched_filename: None,
            file_size_bytes: None,
            sha256: None,
        };
        SystemBiosStatus::from_requirements(
            if required {
                BiosPolicy::Required
            } else {
                BiosPolicy::Optional
            },
            vec![requirement],
        )
    }

    #[test]
    fn available_core_and_no_bios_requirement_is_ready() {
        let (system, core_id) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::Ready,
            &BTreeSet::from([core_id]),
            &SystemBiosStatus::from_requirements(BiosPolicy::NotRequired, Vec::new()),
        );

        assert!(readiness.ready);
        assert!(readiness.reasons.is_empty());
    }

    #[test]
    fn unavailable_core_is_not_ready() {
        let (system, _) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::Ready,
            &BTreeSet::new(),
            &SystemBiosStatus::from_requirements(BiosPolicy::NotRequired, Vec::new()),
        );

        assert!(!readiness.ready);
        assert!(matches!(
            readiness.reasons.as_slice(),
            [ReadinessReason::MissingCore { .. }]
        ));
    }

    #[test]
    fn unavailable_runtime_is_not_ready_even_with_an_available_core() {
        let (system, core_id) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::NotInstalled,
            &BTreeSet::from([core_id]),
            &SystemBiosStatus::from_requirements(BiosPolicy::NotRequired, Vec::new()),
        );

        assert!(!readiness.ready);
        assert!(matches!(
            readiness.reasons.as_slice(),
            [ReadinessReason::RuntimeUnavailable {
                state: RuntimeState::NotInstalled
            }]
        ));
    }

    #[test]
    fn missing_required_bios_is_not_ready() {
        let (system, core_id) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::Ready,
            &BTreeSet::from([core_id]),
            &bios_status(BiosRequirementStatusState::Missing, true),
        );

        assert!(!readiness.ready);
        assert!(readiness
            .reasons
            .iter()
            .any(|reason| matches!(reason, ReadinessReason::MissingRequiredBios { .. })));
    }

    #[test]
    fn valid_required_bios_is_ready() {
        let (system, core_id) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::Ready,
            &BTreeSet::from([core_id]),
            &bios_status(BiosRequirementStatusState::PresentValid, true),
        );

        assert!(readiness.ready);
    }

    #[test]
    fn optional_missing_bios_does_not_block_readiness() {
        let (system, core_id) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::Ready,
            &BTreeSet::from([core_id]),
            &bios_status(BiosRequirementStatusState::OptionalMissing, false),
        );

        assert!(readiness.ready);
    }

    #[test]
    fn invalid_required_bios_is_not_ready() {
        let (system, core_id) = resolved_system();

        let readiness = SystemReadiness::evaluate(
            &system,
            RuntimeState::Ready,
            &BTreeSet::from([core_id]),
            &bios_status(BiosRequirementStatusState::PresentInvalid, true),
        );

        assert!(!readiness.ready);
        assert!(readiness
            .reasons
            .iter()
            .any(|reason| matches!(reason, ReadinessReason::InvalidRequiredBios { .. })));
    }

    #[test]
    fn readiness_reason_ipc_fields_use_camel_case() {
        let reason = ReadinessReason::MissingRequiredBios {
            requirement_id: BiosRequirementId::new("synthetic-bios").unwrap(),
        };
        let serialized = serde_json::to_value(reason).unwrap();

        assert_eq!(serialized["requirementId"], "synthetic-bios");
        assert!(serialized.get("requirement_id").is_none());
    }
}
