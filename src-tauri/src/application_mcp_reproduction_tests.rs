// Genuine MCP stdio reproduction on the same catalog authority as UI writes.

#[test]
fn native_reproduction_stdio_shares_ui_authority_and_replays_without_live_write() {
    let host = Host::catalog();
    host.subscribe();
    let mut client = Client::start(&host);
    let control = |id, operation, payload| host.request(id, operation, payload, None);
    let opted = client.call(&control("opt", "command.reproduction.opt_in", json!({})));
    assert_eq!(opted["result"]["state"], "recording", "{opted}");
    assert!(
        host.state
            .dispatch_native(host.set("ui-40", 0, 40))
            .unwrap()
            .ok
    );
    assert_eq!(client.call(&host.set("mcp-60", 1, 60))["ok"], true);
    assert_eq!(
        client.call(&host.set("mcp-noop-60", 2, 60))["result"]["applied"],
        false
    );
    let status = client.call(&control("status", "query.reproduction.status", json!({})));
    assert_eq!(status["result"]["commandCount"], 3);
    let exported = client.call(&control("export", "query.reproduction.export", json!({})));
    assert_eq!(exported["ok"], true);
    let bundle = exported["result"]["bundle"].clone();
    assert!(bundle.is_object());
    let before = host
        .state
        .dispatch_native(host.request(
            "before",
            "query.document.serialize",
            json!({"atRevision":2}),
            None,
        ))
        .unwrap();
    let trace = client.call(&host.query());
    let report_query = control(
        "report",
        "query.reproduction.report",
        json!({
            "expectedContentRevision":2,"expectedSequence":3
        }),
    );
    let report = client.call(&report_query);
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(
        report["result"],
        json!({"bundle":bundle,
        "verifiedContentRevision":2,"verifiedSequence":3})
    );
    assert_eq!(client.call(&host.query())["result"], trace["result"]);
    let replay = client.call(&control(
        "replay",
        "query.reproduction.replay",
        json!({"bundle":bundle}),
    ));
    assert_eq!(replay["ok"], true);
    assert_eq!(
        replay["result"],
        json!({
            "initial":{"revision":0,"opacity":25,"undoDepth":0,"redoDepth":0},
            "steps":[
                {"id":1,"expectedRevision":0,"applied":true,"state":{"revision":1,"opacity":40,"undoDepth":1,"redoDepth":0}},
                {"id":2,"expectedRevision":1,"applied":true,"state":{"revision":2,"opacity":60,"undoDepth":2,"redoDepth":0}},
                {"id":3,"expectedRevision":2,"applied":false,"state":{"revision":2,"opacity":60,"undoDepth":2,"redoDepth":0}}
            ]
        })
    );
    let after = host
        .state
        .dispatch_native(host.request(
            "after",
            "query.document.serialize",
            json!({"atRevision":2}),
            None,
        ))
        .unwrap();
    assert_eq!(after.result, before.result);
    assert_eq!(after.content_revision, before.content_revision);
    assert_eq!(after.document_id, before.document_id);
    assert_eq!(client.call(&host.query())["result"], trace["result"]);
    assert_eq!(
        client.call(&control(
            "reject",
            "query.reproduction.replay",
            json!({"bundle":{"formatVersion":99}})
        ))["error"]["code"],
        "malformed_payload"
    );
}

#[test]
fn native_reproduction_stdio_preserves_adjacent_float_values_through_export_and_replay() {
    let host = Host::catalog();
    host.subscribe();
    let mut client = Client::start(&host);
    let control = |id, operation, payload| host.request(id, operation, payload, None);
    assert_eq!(
        client.call(&control("opt", "command.reproduction.opt_in", json!({})))["ok"],
        true
    );
    let tiny: f64 = "1.2500000000000003e-300".parse().unwrap();
    let next: f64 = "1.2500000000000005e-300".parse().unwrap();
    assert_ne!(tiny, next);
    for (index, value) in [tiny, next, tiny].into_iter().enumerate() {
        let request = host.request(
            &format!("write-{index}"),
            "command.document.apply",
            json!({"command":"layer.opacity.set","stableTarget":{"layerUid":"r08_curve_layer"},"value":value}),
            Some(index as u64),
        );
        let response = client.call(&request);
        assert_eq!(response["result"]["applied"], true, "{response}");
    }
    let exported = client.call(&control("export", "query.reproduction.export", json!({})));
    assert_eq!(exported["ok"], true, "{exported}");
    let bundle = exported["result"]["bundle"].clone();
    let commands = bundle["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 3);
    for (command, expected) in commands.iter().zip([tiny, next, tiny]) {
        assert_eq!(command["value"].as_f64(), Some(expected), "{bundle}");
    }
    let report = client.call(&control(
        "report",
        "query.reproduction.report",
        json!({
            "expectedContentRevision":3,"expectedSequence":3
        }),
    ));
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["result"]["bundle"], bundle);
    let replay = client.call(&control(
        "replay",
        "query.reproduction.replay",
        json!({"bundle":bundle}),
    ));
    assert_eq!(replay["ok"], true, "{replay}");
    let steps = replay["result"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    for (step, expected) in steps.iter().zip([tiny, next, tiny]) {
        assert_eq!(step["state"]["opacity"].as_f64(), Some(expected));
    }
}

#[test]
fn native_reproduction_stdio_report_checks_live_tokens_and_invalid_journal_without_effects() {
    let host = Host::catalog();
    host.subscribe();
    let mut client = Client::start(&host);
    let control = |id, operation, payload| host.request(id, operation, payload, None);
    assert_eq!(
        client.call(&control("opt", "command.reproduction.opt_in", json!({})))["ok"],
        true
    );
    let first = host.set("first", 0, 40);
    assert_eq!(client.call(&first)["ok"], true);
    let report = |revision, sequence| {
        control(
            "report",
            "query.reproduction.report",
            json!({
                "expectedContentRevision":revision,"expectedSequence":sequence
            }),
        )
    };
    assert_eq!(client.call(&report(1, 1))["ok"], true);
    assert_eq!(
        client.call(&host.set("noop", 1, 40))["result"]["applied"],
        false
    );
    assert_eq!(client.call(&report(1, 1))["error"]["code"], "busy_conflict");
    assert_eq!(
        client.call(&report(0, 2))["error"]["code"],
        "stale_revision"
    );
    assert_eq!(client.call(&first)["ok"], true);
    assert_eq!(client.call(&report(1, 2))["error"]["code"], "busy_conflict");
    let accepted = client.call(&report(1, 3));
    assert_eq!(accepted["ok"], true, "{accepted}");
    assert_eq!(
        accepted["result"]["bundle"]["commands"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let trace = client.call(&host.query());
    let malformed = control(
        "malformed",
        "query.reproduction.report",
        json!({
            "expectedContentRevision":1,"expectedSequence":3,"extra":true
        }),
    );
    assert_eq!(
        client.call(&malformed)["error"]["code"],
        "malformed_payload"
    );
    let mut collision = report(1, 3);
    collision.request_id = "first".into();
    assert_eq!(client.call(&collision)["error"]["code"], "invalid_request");
    assert_eq!(client.call(&report(1, 3))["result"], accepted["result"]);
    assert_eq!(client.call(&host.query())["result"], trace["result"]);
    assert_eq!(
        client.call(&control(
            "job",
            "job.export.png.status",
            json!({"jobId":"absent"})
        ))["ok"],
        false
    );
    let rejected = client.call(&report(1, 3));
    assert_eq!(rejected["error"]["code"], "unavailable");
    assert!(rejected.get("result").is_none());
    assert_eq!(client.call(&host.query())["result"], trace["result"]);
    let ordinary = Host::start();
    let mut ordinary_client = Client::start(&ordinary);
    let denied = ordinary_client.call(&ordinary.request(
        "report",
        "query.reproduction.report",
        json!({"expectedContentRevision":0,"expectedSequence":0}),
        None,
    ));
    assert_eq!(denied["error"]["code"], "unavailable");
    assert!(denied.get("result").is_none());
}
