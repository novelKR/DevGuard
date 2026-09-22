use devguard_contract::*;
use std::collections::{BTreeMap, BTreeSet};

fn intent() -> ResourceIntent {
    ResourceIntent {
        profile: "interactive".into(),
        requested: Budget {
            cpu_milli: 1_000,
            memory_bytes: 2_147_483_648,
            tasks: 64,
        },
        minimum: ResourceLevels::MACOS,
    }
}

#[test]
fn meaning_digest_covers_execution_semantics_and_has_deterministic_environment_order() {
    let meaning = ExecutionMeaning {
        executable_identity: "cargo-sha256".into(),
        cwd_identity: "registered-root-1".into(),
        argv: vec!["cargo".into(), "test".into()],
        environment_changes: BTreeMap::from([("B".into(), "2".into()), ("A".into(), "1".into())]),
        tty: false,
        timeout_ms: 30_000,
        resources: intent(),
    };
    let baseline = meaning.digest().unwrap();
    let mut reordered = meaning.clone();
    reordered.environment_changes =
        BTreeMap::from([("A".into(), "1".into()), ("B".into(), "2".into())]);
    assert_eq!(baseline, reordered.digest().unwrap());
    let mut variants = Vec::new();
    let mut changed = meaning.clone();
    changed.argv.push("--release".into());
    variants.push(changed);
    let mut changed = meaning.clone();
    changed.cwd_identity = "another-root".into();
    variants.push(changed);
    let mut changed = meaning.clone();
    changed.tty = true;
    variants.push(changed);
    let mut changed = meaning.clone();
    changed.timeout_ms += 1;
    variants.push(changed);
    let mut changed = meaning.clone();
    changed
        .environment_changes
        .insert("A".into(), "changed".into());
    variants.push(changed);
    let mut changed = meaning;
    changed.resources.requested.tasks += 1;
    variants.push(changed);
    for variant in variants {
        assert_ne!(baseline, variant.digest().unwrap());
    }
}

#[test]
fn overflowing_reservations_and_zero_workloads_fail_instead_of_wrapping() {
    let largest = Budget {
        cpu_milli: u64::MAX,
        memory_bytes: u64::MAX,
        tasks: u64::MAX,
    };
    assert!(largest.checked_add(intent().requested).is_err());
    assert!(largest.checked_mul(2).is_err());
    assert!(Budget::ZERO.validate_workload().is_err());
    assert_eq!(Budget::ZERO.remaining_after(largest), Budget::ZERO);
}

#[test]
fn compatibility_requires_both_protocol_and_resource_capabilities() {
    let provided = BTreeSet::from([
        Capability::DurableAdmission,
        Capability::FencedLaunch,
        Capability::MacosCooperative,
    ]);
    let old = Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: BTreeSet::from([Capability::DurableAdmission]),
    };
    let new = Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 2,
        required: BTreeSet::from([Capability::DurableAdmission, Capability::FencedLaunch]),
    };
    old.check(1, &provided).unwrap();
    new.check(1, &provided).unwrap();
    assert!(old.check(2, &provided).is_err());
    let kernel = Compatibility {
        required: BTreeSet::from([Capability::LinuxCgroupV2]),
        ..new
    };
    assert!(kernel.check(1, &provided).is_err());
}

#[test]
fn accounting_qos_and_kernel_claims_cannot_be_interchanged() {
    let accounting = PlannedResource {
        level: EnforcementLevel::Accounted,
        method: ControlMethod::Accounting,
    };
    let mut plan = ExecutionPlan {
        scope_kind: ScopeKind::ObservedProcessGroup,
        cpu: PlannedResource {
            level: EnforcementLevel::Cooperative,
            method: ControlMethod::QosAndPriority,
        },
        memory: accounting.clone(),
        pids: accounting,
    };
    plan.validate().unwrap();
    assert!(!plan.levels().satisfies(ResourceLevels::KERNEL));
    plan.memory = plan.cpu.clone();
    assert!(plan.validate().is_err());
    plan.memory = PlannedResource {
        level: EnforcementLevel::Kernel,
        method: ControlMethod::CgroupV2,
    };
    assert!(plan.validate().is_err());
}

#[test]
fn secrets_are_validated_and_debug_redacted() {
    let secret = Secret::new("f".repeat(64)).unwrap();
    assert!(!format!("{secret:?}").contains(secret.expose()));
    assert!(serde_json::from_str::<Secret>("\"short\"").is_err());
    assert_eq!(
        serde_json::from_str::<Secret>(&serde_json::to_string(&secret).unwrap())
            .unwrap()
            .digest(),
        secret.digest()
    );
}

#[test]
fn only_proven_prelaunch_terminal_states_mean_not_started() {
    for phase in [
        AttemptPhase::Denied,
        AttemptPhase::Expired,
        AttemptPhase::Cancelled,
    ] {
        assert!(phase.proven_not_started());
        assert!(!phase.charged());
    }
    for phase in [
        AttemptPhase::LaunchCommitted,
        AttemptPhase::ScopeBound,
        AttemptPhase::RunAuthorized,
        AttemptPhase::Suspect,
        AttemptPhase::Released,
    ] {
        assert!(!phase.proven_not_started());
    }
}

#[test]
fn wire_intent_rejects_unknown_fields_instead_of_ignoring_policy() {
    let mut value = serde_json::to_value(intent()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("force_kernel".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<ResourceIntent>(value).is_err());
}
