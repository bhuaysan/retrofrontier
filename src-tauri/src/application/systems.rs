use crate::application::RuntimeManager;
use crate::domain::bios::{BiosDiscovery, BiosRequirementStatus, BiosRootStatus, SystemBiosStatus};
use crate::domain::core::{CoreId, CorePolicy};
use crate::domain::readiness::SystemReadiness;
use crate::domain::runtime::{RuntimeState, RuntimeStatus};
use crate::domain::system::{SystemCatalog, SystemDefinition, SystemId};
use crate::error::AppError;
use crate::services::bios::BiosService;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone)]
pub struct SystemsApplicationService {
    catalog: SystemCatalog,
    bios: BiosService,
    runtime: RuntimeManager,
}

impl SystemsApplicationService {
    pub fn new(catalog: SystemCatalog, bios: BiosService, runtime: RuntimeManager) -> Self {
        Self {
            catalog,
            bios,
            runtime,
        }
    }

    pub fn get_systems(&self) -> Result<SystemsResponse, AppError> {
        let runtime = self.runtime.status().map_err(AppError::Runtime)?;
        let available_core_ids = self.available_core_ids(&runtime)?;
        let bios = self.bios.discover(None).map_err(AppError::Bios)?;
        Ok(self.build_response(runtime, available_core_ids, bios))
    }

    pub fn get_bios_status(
        &self,
        root_override: Option<PathBuf>,
    ) -> Result<BiosDiscovery, AppError> {
        self.bios
            .discover(root_override.as_deref())
            .map_err(AppError::Bios)
    }

    fn available_core_ids(&self, runtime: &RuntimeStatus) -> Result<BTreeSet<CoreId>, AppError> {
        if !matches!(
            runtime.state,
            RuntimeState::Ready | RuntimeState::RollbackAvailable
        ) {
            return Ok(BTreeSet::new());
        }
        self.runtime
            .current_verified_core_ids()
            .map_err(AppError::Runtime)?
            .into_iter()
            .map(|id| {
                CoreId::new(id.as_str()).map_err(|error| AppError::Catalog(error.to_string()))
            })
            .collect()
    }

    fn build_response(
        &self,
        runtime: RuntimeStatus,
        available_core_ids: BTreeSet<CoreId>,
        bios: BiosDiscovery,
    ) -> SystemsResponse {
        let bios_by_system = bios.requirements.iter().cloned().fold(
            BTreeMap::<SystemId, Vec<_>>::new(),
            |mut grouped, requirement| {
                grouped
                    .entry(requirement.system_id)
                    .or_default()
                    .push(requirement);
                grouped
            },
        );

        let systems = self
            .catalog
            .systems()
            .iter()
            .map(|system| {
                build_system_status(system, &runtime, &available_core_ids, &bios_by_system)
            })
            .collect();

        SystemsResponse {
            runtime,
            bios_root: bios.root,
            bios_root_status: bios.root_status,
            systems,
        }
    }
}

fn build_system_status(
    system: &SystemDefinition,
    runtime: &RuntimeStatus,
    available_core_ids: &BTreeSet<CoreId>,
    bios_by_system: &BTreeMap<SystemId, Vec<BiosRequirementStatus>>,
) -> SystemStatus {
    let bios = SystemBiosStatus::from_requirements(
        system.bios_policy,
        bios_by_system.get(&system.id).cloned().unwrap_or_default(),
    );
    let default_core_available = system
        .core_policy
        .default_core_id
        .as_ref()
        .map(|core_id| available_core_ids.contains(core_id));
    let core = SystemCoreStatus {
        policy: system.core_policy.clone(),
        availability: CoreAvailabilityStatus {
            runtime_state: runtime.state,
            available_core_ids: available_core_ids.iter().cloned().collect(),
            default_core_available,
        },
    };
    let readiness = SystemReadiness::evaluate(system, runtime.state, available_core_ids, &bios);

    SystemStatus {
        id: system.id,
        display_name: system.display_name.clone(),
        manufacturer: system.manufacturer.clone(),
        aliases: system.aliases.clone(),
        supported_extensions: system.supported_extensions.clone(),
        core,
        bios,
        readiness,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemsResponse {
    pub runtime: RuntimeStatus,
    pub bios_root: String,
    pub bios_root_status: BiosRootStatus,
    pub systems: Vec<SystemStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub id: SystemId,
    pub display_name: String,
    pub manufacturer: String,
    pub aliases: Vec<String>,
    pub supported_extensions: Vec<String>,
    pub core: SystemCoreStatus,
    pub bios: SystemBiosStatus,
    pub readiness: SystemReadiness,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCoreStatus {
    pub policy: CorePolicy,
    pub availability: CoreAvailabilityStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreAvailabilityStatus {
    pub runtime_state: RuntimeState,
    pub available_core_ids: Vec<CoreId>,
    pub default_core_available: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::build_system_status;
    use crate::domain::bios::{
        BiosPolicy, BiosRequirementStatus, BiosRequirementStatusState, SystemBiosStatus,
    };
    use crate::domain::core::CoreId;
    use crate::domain::runtime::{RuntimeState, RuntimeStatus};
    use crate::domain::system::{SystemCatalog, SystemId};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn unresolved_policy_is_exposed_separately_from_runtime_core_availability() {
        let catalog = SystemCatalog::v1();
        let system = catalog.system(SystemId::Nes).unwrap();
        let runtime = RuntimeStatus {
            state: RuntimeState::Ready,
            installation_id: Some("install-1".to_owned()),
            release_id: Some("release-1".to_owned()),
            can_rollback: false,
            repair_required: false,
        };
        let available = BTreeSet::from([CoreId::new("unapproved-core").unwrap()]);

        let status = build_system_status(system, &runtime, &available, &BTreeMap::new());

        assert!(status.core.availability.default_core_available.is_none());
        assert!(status.core.policy.default_core_id.is_none());
        assert!(!status.readiness.ready);
        assert!(status.readiness.reasons.iter().any(|reason| matches!(
            reason,
            crate::domain::readiness::ReadinessReason::CorePolicyUnresolved { .. }
        )));
    }

    #[test]
    fn required_invalid_bios_is_not_ready_even_when_runtime_is_ready() {
        let catalog = SystemCatalog::v1();
        let system = catalog.system(SystemId::PlayStation).unwrap();
        let runtime = RuntimeStatus {
            state: RuntimeState::Ready,
            installation_id: Some("install-1".to_owned()),
            release_id: Some("release-1".to_owned()),
            can_rollback: false,
            repair_required: false,
        };
        let requirement = BiosRequirementStatus {
            requirement_id: system.bios_requirements[0].id.clone(),
            system_id: SystemId::PlayStation,
            required: true,
            state: BiosRequirementStatusState::PresentInvalid,
            expected_filenames: vec!["firmware.bin".to_owned()],
            expected_size_bytes: None,
            description: "synthetic".to_owned(),
            matched_filename: Some("firmware.bin".to_owned()),
            file_size_bytes: Some(3),
            sha256: Some("0".repeat(64)),
        };
        let bios = SystemBiosStatus::from_requirements(BiosPolicy::Required, vec![requirement]);
        let mut grouped = BTreeMap::new();
        grouped.insert(SystemId::PlayStation, bios.requirements.clone());

        let status = build_system_status(system, &runtime, &BTreeSet::new(), &grouped);

        assert!(!status.bios.ready);
        assert!(status.readiness.reasons.iter().any(|reason| matches!(
            reason,
            crate::domain::readiness::ReadinessReason::InvalidRequiredBios { .. }
        )));
    }
}
