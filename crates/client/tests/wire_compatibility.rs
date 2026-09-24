use devguard_client::protocol::{Frame, Request, Response, WIRE_VERSION};
use serde_json::{json, Value};

fn current_hello() -> Value {
    json!({
        "version": 1,
        "request_id": 1,
        "body": {
            "method": "hello",
            "params": {
                "compatibility": {
                    "minimum_protocol": 1,
                    "maximum_protocol": 1,
                    "required": []
                }
            }
        }
    })
}

#[test]
fn current_request_fixture_decodes_but_additive_fields_do_not_imply_compatibility() {
    let current = current_hello();
    let frame: Frame<Request> = serde_json::from_value(current.clone()).unwrap();
    assert_eq!(frame.version, WIRE_VERSION);
    assert_eq!(frame.request_id, 1);
    assert!(matches!(frame.body, Request::Hello { .. }));
    for pointer in ["", "/body", "/body/params", "/body/params/compatibility"] {
        let mut future = current.clone();
        future
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("future_field".into(), json!(true));
        assert!(
            serde_json::from_value::<Frame<Request>>(future).is_err(),
            "accepted {pointer}"
        );
    }
    let mut unsupported_capability = current;
    unsupported_capability["body"]["params"]["compatibility"]["required"] =
        json!(["future_capability"]);
    assert!(serde_json::from_value::<Frame<Request>>(unsupported_capability).is_err());
}

#[test]
fn caller_cannot_add_a_peer_identity_or_change_the_credential_role_shape() {
    let current = json!({
        "version": 1,
        "request_id": 2,
        "body": {"method": "authenticate", "params": {"credential": {
            "kind": "consumer", "consumer_id": "consumer", "generation": "generation",
            "secret": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        }}}
    });
    assert!(serde_json::from_value::<Frame<Request>>(current.clone()).is_ok());
    for field in ["uid", "pid", "role", "permit", "authority"] {
        let mut changed = current.clone();
        changed["body"]["params"]["credential"][field] = json!(1);
        assert!(
            serde_json::from_value::<Frame<Request>>(changed).is_err(),
            "accepted {field}"
        );
    }
    let mut helper_role = current;
    helper_role["body"]["params"]["credential"]["kind"] = json!("helper");
    assert!(serde_json::from_value::<Frame<Request>>(helper_role).is_err());
    let forged_register = json!({"version":1,"request_id":3,"body":{
        "method":"register","params":{"instance_id":"instance","pid":123}}});
    assert!(serde_json::from_value::<Frame<Request>>(forged_register).is_err());
}

#[test]
fn response_fixtures_reject_additions_in_envelope_status_identity_and_error() {
    let hello = json!({"version":1,"request_id":1,"body":{"result":"hello","value":{
        "protocol":1,"authority":{"uid":501,"pid":100},"caller":{"uid":501,"pid":200},
        "capabilities":[],"max_frame_bytes":65536,"frame_deadline_ms":250,"max_sessions":32}}});
    for pointer in [
        "",
        "/body",
        "/body/value",
        "/body/value/authority",
        "/body/value/caller",
    ] {
        assert!(serde_json::from_value::<Frame<Response>>(hello.clone()).is_ok());
        let mut future = hello.clone();
        future
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("future_field".into(), json!(true));
        assert!(
            serde_json::from_value::<Frame<Response>>(future).is_err(),
            "accepted {pointer}"
        );
    }
    let status = json!({"version":1,"request_id":3,"body":{"result":"status","value":{
        "storage_validated":true,"registration_ready":false,"execution_ready":false,
        "reason":"native evidence is unavailable","configuration_fingerprint":"fixture"}}});
    let error = json!({"version":1,"request_id":3,"body":{"result":"error","value":{
        "code":"resource_control_unavailable","message":"not ready"}}});
    for current in [status, error] {
        assert!(serde_json::from_value::<Frame<Response>>(current.clone()).is_ok());
        let mut future = current;
        future["body"]["value"]["future_field"] = json!(true);
        assert!(serde_json::from_value::<Frame<Response>>(future).is_err());
    }
}

const PERMIT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn key() -> Value {
    json!({"consumer_id": "consumer", "consumer_generation": "generation", "attempt_id": "attempt"})
}

fn attempt() -> devguard_contract::AttemptRecord {
    use devguard_contract::*;
    AttemptRecord {
        key: serde_json::from_value(key()).unwrap(),
        request_fingerprint: digest_bytes(b"fingerprint"),
        owner: InstanceIdentity {
            instance_id: "instance".into(),
            process: ProcessIdentity {
                boot_id: "boot".into(),
                pid: 10,
                start_ticks: 20,
            },
        },
        policy_revision: "policy".into(),
        phase: AttemptPhase::LaunchCommitted,
        reservation: Some(ResourceReservation {
            lease_id: "lease".into(),
            quantities: Budget {
                cpu_milli: 1_000,
                memory_bytes: 1,
                tasks: 1,
            },
            prepared_at: ObservationTime {
                boot_id: "boot".into(),
                monotonic_ms: 1,
            },
            prepare_deadline_ms: 5_001,
        }),
        plan: None,
        scope: None,
        applied: None,
        denial: None,
        tracking_lost: false,
        release_reason: None,
    }
}

#[test]
fn launch_requests_decode_strictly_and_carry_no_process_identity() {
    let intent = json!({"profile": "interactive",
        "requested": {"cpu_milli": 1000, "memory_bytes": 1, "tasks": 1},
        "minimum": {"cpu": "cooperative", "memory": "accounted", "pids": "accounted"}});
    let digest = devguard_contract::digest_bytes(b"meaning");
    let requests = [
        json!({"method": "admit", "params": {"request":
            {"key": key(), "execution_digest": digest, "intent": intent}}}),
        json!({"method": "begin_launch", "params": {"key": key()}}),
        json!({"method": "lookup", "params": {"key": key()}}),
        json!({"method": "cancel", "params": {"key": key()}}),
        json!({"method": "launch", "params":
            {"key": key(), "instance_id": "instance", "permit": PERMIT}}),
    ];
    for body in requests {
        let frame = json!({"version": 1, "request_id": 4, "body": body});
        assert!(
            serde_json::from_value::<Frame<Request>>(frame.clone()).is_ok(),
            "{frame}"
        );
        // Neither a caller nor a helper can declare a process identity.
        for field in ["pid", "uid", "helper", "future_field"] {
            let mut changed = frame.clone();
            changed["body"]["params"][field] = json!(1);
            assert!(
                serde_json::from_value::<Frame<Request>>(changed).is_err(),
                "accepted {field} in {frame}"
            );
        }
        let mut changed_key = frame.clone();
        if changed_key["body"]["params"].get("key").is_some() {
            changed_key["body"]["params"]["key"]["pid"] = json!(1);
            assert!(serde_json::from_value::<Frame<Request>>(changed_key).is_err());
        }
    }
    let short = json!({"version": 1, "request_id": 5, "body": {"method": "launch",
        "params": {"key": key(), "instance_id": "instance", "permit": "abc"}}});
    assert!(serde_json::from_value::<Frame<Request>>(short).is_err());
}

#[test]
fn launch_responses_decode_strictly_and_never_print_the_permit() {
    use devguard_client::protocol::{LaunchAuthorization, LaunchGrant};
    let record = serde_json::to_value(attempt()).unwrap();
    let responses = [
        json!({"result": "attempt", "value": record}),
        json!({"result": "launch_granted", "value": {"attempt": record, "permit": PERMIT}}),
        json!({"result": "launch_granted", "value": {"attempt": record, "permit": null}}),
        json!({"result": "launch_authorized", "value": {"attempt": record, "may_exec": true}}),
    ];
    for body in responses {
        let frame = json!({"version": 1, "request_id": 6, "body": body});
        assert!(
            serde_json::from_value::<Frame<Response>>(frame.clone()).is_ok(),
            "{frame}"
        );
        let mut future = frame.clone();
        future["body"]["value"]["future_field"] = json!(true);
        assert!(serde_json::from_value::<Frame<Response>>(future).is_err());
    }
    let granted = Response::LaunchGranted(LaunchGrant {
        attempt: attempt(),
        permit: Some(devguard_contract::Secret::new(PERMIT.into()).unwrap()),
    });
    assert!(!format!("{granted:?}").contains(PERMIT));
    let authorized = Response::LaunchAuthorized(LaunchAuthorization {
        attempt: attempt(),
        may_exec: false,
    });
    assert!(format!("{authorized:?}").contains("may_exec: false"));
}

#[test]
fn reconcile_requests_decode_strictly_and_carry_no_process_identity() {
    let requests = [
        json!({"method": "abandon_launch", "params": {"key": key(), "reason": "spawn_failed"}}),
        json!({"method": "abandon_launch", "params": {"key": key(), "reason": "helper_exited"}}),
        json!({"method": "abandon_launch", "params": {"key": key(), "reason": "grant_not_received"}}),
        json!({"method": "observe", "params": {"key": key()}}),
        json!({"method": "terminate", "params": {"key": key(), "signal": "terminate"}}),
        json!({"method": "terminate", "params": {"key": key(), "signal": "kill"}}),
    ];
    for body in requests {
        let frame = json!({"version": 1, "request_id": 7, "body": body});
        assert!(
            serde_json::from_value::<Frame<Request>>(frame.clone()).is_ok(),
            "{frame}"
        );
        for field in ["pid", "uid", "evidence", "future_field"] {
            let mut changed = frame.clone();
            changed["body"]["params"][field] = json!(1);
            assert!(
                serde_json::from_value::<Frame<Request>>(changed).is_err(),
                "accepted {field} in {frame}"
            );
        }
    }
    // Unknown reasons and signals, including raw numbers, are refused.
    for (method, field, value) in [
        ("abandon_launch", "reason", json!("process_died")),
        ("terminate", "signal", json!("stop")),
        ("terminate", "signal", json!(9)),
    ] {
        let mut params = json!({"key": key()});
        params[field] = value;
        let frame =
            json!({"version": 1, "request_id": 8, "body": {"method": method, "params": params}});
        assert!(serde_json::from_value::<Frame<Request>>(frame).is_err());
    }
    let terminated = json!({"version": 1, "request_id": 9, "body": {"result": "terminated",
        "value": {"attempt": serde_json::to_value(attempt()).unwrap(), "signalled": 2, "complete": true}}});
    assert!(serde_json::from_value::<Frame<Response>>(terminated.clone()).is_ok());
    let mut future = terminated;
    future["body"]["value"]["future_field"] = json!(true);
    assert!(serde_json::from_value::<Frame<Response>>(future).is_err());
}

fn lease() -> devguard_contract::LeaseRecord {
    use devguard_contract::*;
    let attempt = attempt();
    LeaseRecord {
        key: attempt.key,
        owner: attempt.owner,
        policy_revision: "policy".into(),
        budget: Budget {
            cpu_milli: 2_000,
            memory_bytes: 1,
            tasks: 1,
        },
        phase: LeasePhase::Active,
        created_at: ObservationTime {
            boot_id: "boot".into(),
            monotonic_ms: 1,
        },
        deadline_ms: Some(60_001),
        end_reason: None,
    }
}

#[test]
fn lease_requests_decode_strictly_and_never_print_the_token() {
    use devguard_client::protocol::LeaseGranted;
    let intent = json!({"profile": "interactive",
        "requested": {"cpu_milli": 1000, "memory_bytes": 1, "tasks": 1},
        "minimum": {"cpu": "cooperative", "memory": "accounted", "pids": "accounted"}});
    let budget = json!({"cpu_milli": 2000, "memory_bytes": 1, "tasks": 1});
    let child = json!({"key": key(), "execution_digest": devguard_contract::digest_bytes(b"child"),
                       "intent": intent});
    let requests = [
        json!({"method": "admit_lease", "params": {"key": key(), "budget": budget, "ttl_ms": 60000}}),
        json!({"method": "admit_lease", "params": {"key": key(), "budget": budget, "ttl_ms": null}}),
        json!({"method": "admit_child", "params": {"lease": key(), "token": PERMIT, "request": child}}),
        json!({"method": "end_lease", "params": {"key": key()}}),
        json!({"method": "lease_status", "params": {"key": key(), "token": PERMIT}}),
    ];
    for body in requests {
        let frame = json!({"version": 1, "request_id": 10, "body": body});
        assert!(
            serde_json::from_value::<Frame<Request>>(frame.clone()).is_ok(),
            "{frame}"
        );
        // No holder can declare an identity, a phase or a larger lease.
        for field in ["pid", "owner", "phase", "remaining", "future_field"] {
            let mut changed = frame.clone();
            changed["body"]["params"][field] = json!(1);
            assert!(
                serde_json::from_value::<Frame<Request>>(changed).is_err(),
                "accepted {field} in {frame}"
            );
        }
    }
    for method in ["admit_child", "lease_status"] {
        let params = if method == "admit_child" {
            json!({"lease": key(), "token": "abc", "request": child})
        } else {
            json!({"key": key(), "token": "abc"})
        };
        let short =
            json!({"version": 1, "request_id": 11, "body": {"method": method, "params": params}});
        assert!(serde_json::from_value::<Frame<Request>>(short).is_err());
    }
    let record = serde_json::to_value(lease()).unwrap();
    let responses = [
        json!({"result": "lease_granted", "value": {"lease": record, "token": PERMIT}}),
        json!({"result": "lease_granted", "value": {"lease": record, "token": null}}),
        json!({"result": "lease", "value": {"lease": record,
            "remaining": {"cpu_milli": 1000, "memory_bytes": 1, "tasks": 1}, "children": [key()]}}),
    ];
    for body in responses {
        let frame = json!({"version": 1, "request_id": 12, "body": body});
        assert!(
            serde_json::from_value::<Frame<Response>>(frame.clone()).is_ok(),
            "{frame}"
        );
        let mut future = frame.clone();
        future["body"]["value"]["future_field"] = json!(true);
        assert!(serde_json::from_value::<Frame<Response>>(future).is_err());
        let mut inner = frame;
        inner["body"]["value"]["lease"]["future_field"] = json!(true);
        assert!(serde_json::from_value::<Frame<Response>>(inner).is_err());
    }
    let granted = Response::LeaseGranted(LeaseGranted {
        lease: lease(),
        token: Some(devguard_contract::Secret::new(PERMIT.into()).unwrap()),
    });
    assert!(!format!("{granted:?}").contains(PERMIT));
    // The capability added after protocol 1 has a stable name.
    assert_eq!(
        serde_json::to_value(devguard_contract::Capability::ParentLease).unwrap(),
        json!("parent_lease")
    );
}
