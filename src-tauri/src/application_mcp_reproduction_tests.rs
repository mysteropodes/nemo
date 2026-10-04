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
