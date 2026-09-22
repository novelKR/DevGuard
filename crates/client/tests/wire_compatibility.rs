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
