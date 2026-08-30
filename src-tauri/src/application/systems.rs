use crate::application::RuntimeApplicationService;
use crate::domain::bios::{BiosDiscovery, BiosRequirementStatus, BiosRootStatus, SystemBiosStatus};
use crate::domain::core::{CoreId, CorePolicy};
use crate::domain::readiness::SystemReadiness;
use crate::domain::runtime::{RuntimeState, RuntimeStatus, VerifiedRuntimeSnapshot};
use crate::domain::system::{SystemCatalog, SystemDefinition, SystemId};
use crate::error::AppError;
use crate::services::bios::BiosService;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

trait RuntimeSnapshotReader: Send + Sync {
    fn verified_runtime_snapshot(&self) -> Result<VerifiedRuntimeSnapshot, AppError>;
}

impl RuntimeSnapshotReader for RuntimeApplicationService {
    fn verified_runtime_snapshot(&self) -> Result<VerifiedRuntimeSnapshot, AppError> {
        RuntimeApplicationService::verified_runtime_snapshot(self)
    }
}

#[derive(Clone)]
pub struct SystemsApplicationService {
    catalog: SystemCatalog,
    bios: BiosService,
    runtime: Arc<dyn RuntimeSnapshotReader>,
}

impl SystemsApplicationService {
    pub fn new(
        catalog: SystemCatalog,
        bios: BiosService,
        runtime: RuntimeApplicationService,
    ) -> Self {
        Self {
            catalog,
            bios,
            runtime: Arc::new(runtime),
        }
    }

    #[cfg(test)]
    fn with_runtime_reader(
        catalog: SystemCatalog,
        bios: BiosService,
        runtime: Arc<dyn RuntimeSnapshotReader>,
    ) -> Self {
        Self {
            catalog,
            bios,
            runtime,
        }
    }

    pub fn get_systems(&self) -> Result<SystemsResponse, AppError> {
        let snapshot = self.runtime.verified_runtime_snapshot()?;
        let available_core_ids = self.available_core_ids(&snapshot)?;
        let runtime = snapshot.status;
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

    /// Translate verified managed component identifiers into approved catalog cores.
    ///
    /// RuntimeManager reports which authenticated components are installed; the catalog decides
    /// which of them RetroFrontier approves. A component with no approved definition, or one whose
    /// definition does not declare the running platform target, is never reported as available.
    fn available_core_ids(
        &self,
        snapshot: &VerifiedRuntimeSnapshot,
    ) -> Result<BTreeSet<CoreId>, AppError> {
        if !matches!(
            snapshot.status.state,
            RuntimeState::Ready | RuntimeState::RollbackAvailable
        ) {
            return Ok(BTreeSet::new());
        }
        Ok(snapshot
            .verified_core_ids
            .iter()
            .filter_map(|component_id| self.catalog.core_for_component(component_id))
            .filter(|core| core.supports_current_target())
            .map(|core| core.id.clone())
            .collect())
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
    use super::RuntimeSnapshotReader;
    use super::SystemsApplicationService;
    use crate::domain::bios::{
        BiosPolicy, BiosRequirementStatus, BiosRequirementStatusState, SystemBiosStatus,
    };
    use crate::domain::core::CoreId;
    use crate::domain::runtime::{RuntimeState, RuntimeStatus, VerifiedRuntimeSnapshot};
    use crate::domain::system::{SystemCatalog, SystemId};
    use crate::services::bios::BiosService;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn unresolved_policy_is_exposed_separately_from_runtime_core_availability() {
        let catalog = SystemCatalog::v1();
        let system = catalog.system(SystemId::Nintendo64).unwrap();
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

    struct CountingSnapshotReader {
        calls: AtomicUsize,
        snapshot: VerifiedRuntimeSnapshot,
    }

    impl RuntimeSnapshotReader for CountingSnapshotReader {
        fn verified_runtime_snapshot(
            &self,
        ) -> Result<VerifiedRuntimeSnapshot, crate::error::AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.snapshot.clone())
        }
    }

    #[test]
    fn only_approved_managed_components_become_available_cores() {
        let catalog = SystemCatalog::v1();
        let bios = BiosService::from_catalog("/tmp/retrofrontier-m7-test-bios", &catalog).unwrap();
        let reader = Arc::new(CountingSnapshotReader {
            calls: AtomicUsize::new(0),
            snapshot: VerifiedRuntimeSnapshot {
                status: RuntimeStatus {
                    state: RuntimeState::Ready,
                    installation_id: Some("install-1".to_owned()),
                    release_id: Some("release-1".to_owned()),
                    can_rollback: false,
                    repair_required: false,
                },
                verified_core_ids: BTreeSet::from([
                    "nestopia".try_into().unwrap(),
                    "some-unapproved-core".try_into().unwrap(),
                ]),
            },
        });
        let service = SystemsApplicationService::with_runtime_reader(catalog, bios, reader);

        let response = service.get_systems().unwrap();
        let nes = response
            .systems
            .iter()
            .find(|system| system.id == SystemId::Nes)
            .unwrap();

        assert_eq!(
            nes.core.availability.available_core_ids,
            vec![CoreId::new("nestopia").unwrap()]
        );
        assert_eq!(nes.core.availability.default_core_available, Some(true));
        assert!(nes.readiness.ready);

        let nintendo_64 = response
            .systems
            .iter()
            .find(|system| system.id == SystemId::Nintendo64)
            .unwrap();
        assert!(nintendo_64
            .core
            .availability
            .default_core_available
            .is_none());
        assert!(!nintendo_64.readiness.ready);
    }

    #[test]
    fn a_resolved_system_without_its_installed_core_reports_a_missing_core() {
        let catalog = SystemCatalog::v1();
        let bios = BiosService::from_catalog("/tmp/retrofrontier-m7-test-bios", &catalog).unwrap();
        let reader = Arc::new(CountingSnapshotReader {
            calls: AtomicUsize::new(0),
            snapshot: VerifiedRuntimeSnapshot {
                status: RuntimeStatus {
                    state: RuntimeState::Ready,
                    installation_id: Some("install-1".to_owned()),
                    release_id: Some("release-1".to_owned()),
                    can_rollback: false,
                    repair_required: false,
                },
                verified_core_ids: BTreeSet::new(),
            },
        });
        let service = SystemsApplicationService::with_runtime_reader(catalog, bios, reader);

        let response = service.get_systems().unwrap();
        let snes = response
            .systems
            .iter()
            .find(|system| system.id == SystemId::Snes)
            .unwrap();

        assert_eq!(snes.core.availability.default_core_available, Some(false));
        assert!(snes.readiness.reasons.iter().any(|reason| matches!(
            reason,
            crate::domain::readiness::ReadinessReason::MissingCore { .. }
        )));
    }

    #[test]
    fn systems_query_consumes_one_coherent_runtime_snapshot() {
        let catalog = SystemCatalog::v1();
        let bios = BiosService::from_catalog("/tmp/retrofrontier-m4-test-bios", &catalog).unwrap();
        let reader = Arc::new(CountingSnapshotReader {
            calls: AtomicUsize::new(0),
            snapshot: VerifiedRuntimeSnapshot {
                status: RuntimeStatus {
                    state: RuntimeState::Ready,
                    installation_id: Some("install-1".to_owned()),
                    release_id: Some("release-1".to_owned()),
                    can_rollback: false,
                    repair_required: false,
                },
                verified_core_ids: BTreeSet::new(),
            },
        });
        let service = SystemsApplicationService::with_runtime_reader(catalog, bios, reader.clone());

        service.get_systems().unwrap();

        assert_eq!(reader.calls.load(Ordering::SeqCst), 1);
    }
}
