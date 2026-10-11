//! Host protocol tests use the real DocumentSession and persistence writer.
use serde_json::{json, Value};
use terraphim_editor::host::{source_hash, HostSession};

fn request(s: &HostSession, id: u64, command: &str, args: Value) -> String {
    let p = s.snapshot();
    json!({"protocol":1,"resource":p["resource"],"session":p["session"],
        "revision":p["revision"],"sourceHash":p["sourceHash"],"id":id,
        "command":command,"args":args})
    .to_string()
}
fn accept(s: &mut HostSession, p: &Value) {
    s.acknowledge(
        p["id"].as_u64().unwrap(),
        p["revision"].as_u64().unwrap() as u32 + 1,
        p["source"].as_str().unwrap(),
    )
    .unwrap();
}
#[test]
fn staged_ghost_is_not_committed_until_exact_host_output_arrives() {
    let mut s = HostSession::new("file:///a.md", "view-a", 1, "Alpha beta.").unwrap();
    let before = s.snapshot();
    let p = s
        .propose(&request(&s, 1, "ghost", json!({"start":6,"end":10})))
        .unwrap();
    assert_eq!(s.snapshot(), before);
    assert_eq!(p["projection"]["body"], "Alpha beta.");
    assert_eq!(
        p["projection"]["annotations"]["ghosts"][0]["anchor"]["text"],
        "beta"
    );
    assert!(s.acknowledge(1, 2, "Alpha WRONG.").is_err());
    assert_eq!(s.snapshot(), before);
    accept(&mut s, &p);
    assert_eq!(s.snapshot()["projection"]["export"], "Alpha.");
    assert!(s.acknowledge(1, 3, p["source"].as_str().unwrap()).is_err());
}
#[test]
fn stale_duplicate_wrong_resource_and_wrong_session_requests_are_refused() {
    let mut s = HostSession::new("file:///a.md", "session-a", 7, "abc").unwrap();
    let base = request(&s, 1, "edit", json!({"start":3,"deleted":0,"inserted":"d"}));
    for (field, value) in [
        ("protocol", json!(2)),
        ("resource", json!("file:///b.md")),
        ("session", json!("session-b")),
        ("revision", json!(6)),
        ("sourceHash", json!("bad")),
    ] {
        let mut bad: Value = serde_json::from_str(&base).unwrap();
        bad[field] = value;
        assert!(s.propose(&bad.to_string()).is_err());
    }
    let p = s.propose(&base).unwrap();
    assert!(s.propose(&base).is_err());
    s.reject(1).unwrap();
    assert!(s.propose(&base).is_err(), "rejected IDs cannot be replayed");
    assert_eq!(s.snapshot()["projection"]["body"], "abc");
    assert_eq!(p["projection"]["body"], "abcd");
}
#[test]
fn sessions_and_external_host_history_remain_isolated() {
    let mut a = HostSession::new("file:///a.md", "a", 1, "a banana").unwrap();
    let b = HostSession::new("file:///b.md", "b", 1, "different").unwrap();
    let p = a
        .propose(&request(
            &a,
            1,
            "alt_create",
            json!({"kind":"word","start":2,"end":8,"text":"apple"}),
        ))
        .unwrap();
    accept(&mut a, &p);
    let annotated = p["source"].as_str().unwrap().to_owned();
    let id = p["projection"]["annotations"]["spans"][0]["id"]
        .as_str()
        .unwrap();
    let p = a
        .propose(&request(&a, 2, "alt_active", json!({"span":id,"index":1})))
        .unwrap();
    assert_eq!(p["projection"]["body"], "an apple");
    accept(&mut a, &p);
    a.synchronize(4, &annotated).unwrap(); // actual host undo
    assert_eq!(a.snapshot()["projection"]["body"], "a banana");
    assert_eq!(
        a.snapshot()["projection"]["annotations"]["spans"][0]["active"],
        0
    );
    assert_eq!(b.snapshot()["projection"]["body"], "different");
    assert_eq!(
        b.snapshot()["projection"]["annotations"]["spans"],
        json!([])
    );
}
#[test]
fn external_edit_invalidates_proposal_and_utf16_offsets_are_preserved() {
    let mut s = HostSession::new("file:///a.md", "a", 1, "😀 Café\r\nTail").unwrap();
    let p = s
        .propose(&request(
            &s,
            1,
            "edit",
            json!({"start":3,"deleted":4,"inserted":"tea"}),
        ))
        .unwrap();
    assert_eq!(p["projection"]["body"], "😀 tea\r\nTail");
    s.synchronize(2, "External change").unwrap();
    assert!(s.acknowledge(1, 3, p["source"].as_str().unwrap()).is_err());
    assert!(s.synchronize(2, "overwrite same revision").is_err());
    assert_eq!(s.snapshot()["projection"]["body"], "External change");
    assert_eq!(
        source_hash("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
#[test]
fn malformed_source_is_preserved_and_invalid_operation_is_atomic() {
    let raw = "Text\n\n```terraphim-alternatives\n{ broken\n```\n";
    let mut s = HostSession::new("file:///a.md", "a", 1, raw).unwrap();
    assert_eq!(s.snapshot()["source"], raw);
    let before = s.snapshot();
    assert!(s
        .propose(&request(&s, 1, "ghost", json!({"start":0,"end":2})))
        .is_err());
    assert_eq!(s.snapshot(), before);
}

#[test]
fn wasm_facade_serializes_real_domain_output_and_refuses_lossy_ids() {
    use terraphim_editor::host::WasmHostSession;
    let mut s = WasmHostSession::new("file:///a.md", "a", 1, "abc").unwrap();
    let before: Value = serde_json::from_str(&s.snapshot()).unwrap();
    let command = json!({"protocol":1,"resource":"file:///a.md","session":"a",
        "revision":1,"sourceHash":before["sourceHash"],"id":1,"command":"edit",
        "args":{"start":3,"deleted":0,"inserted":"d"}})
    .to_string();
    let p: Value = serde_json::from_str(&s.propose(&command).unwrap()).unwrap();
    assert_eq!(p["projection"]["body"], "abcd");
    for id in [f64::NAN, f64::INFINITY, -1.0, 1.5, 9_007_199_254_740_992.0] {
        assert!(s.reject(id).is_err());
    }
    let done: Value = serde_json::from_str(
        &s.acknowledge(1.0, 2, p["source"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(done["projection"]["body"], "abcd");
}

#[test]
fn lf_ui_source_keeps_host_crlf_and_rebases_annotation_offsets() {
    let mut original = HostSession::new("file:///a.md", "original", 1, "First\nbanana").unwrap();
    let p = original
        .propose(&request(
            &original,
            1,
            "alt_create",
            json!({"kind":"word","start":6,"end":12,"text":"apple"}),
        ))
        .unwrap();
    let mut host = HostSession::new("file:///a.md", "host", 1, "First\r\nbanana").unwrap();
    let result = host
        .propose(&request(
            &host,
            1,
            "replace_source",
            json!({"source":p["source"]}),
        ))
        .unwrap();
    let source = result["source"].as_str().unwrap();
    assert!(
        !source.replace("\r\n", "").contains('\n'),
        "native CRLF host must receive uniform CRLF"
    );
    assert_eq!(result["projection"]["body"], "First\r\nbanana");
    assert_eq!(
        result["projection"]["annotations"]["spans"][0]["anchor"]["start"],
        7
    );
    accept(&mut host, &result);
    assert_eq!(host.snapshot()["source"], source);
}

#[test]
fn crlf_conversion_preserves_one_multiline_ghost() {
    let mut local = HostSession::new("file:///a.md", "ui", 1, "Alpha\nbeta").unwrap();
    let p = local
        .propose(&request(&local, 1, "ghost", json!({"start":0,"end":10})))
        .unwrap();
    let mut native = HostSession::new("file:///a.md", "host", 1, "Alpha\r\nbeta").unwrap();
    let result = native
        .propose(&request(
            &native,
            1,
            "replace_source",
            json!({"source":p["source"]}),
        ))
        .unwrap();
    let ghosts = result["projection"]["annotations"]["ghosts"]
        .as_array()
        .unwrap();
    assert_eq!(ghosts.len(), 1);
    assert_eq!(ghosts[0]["anchor"]["text"], "Alpha\r\nbeta");
    assert_eq!(ghosts[0]["anchor"]["end"], 11);
}

#[test]
fn crlf_conversion_keeps_multiline_alternatives_live() {
    let mut ui = HostSession::new("file:///a.md", "ui", 1, "First\nbanana").unwrap();
    let p = ui
        .propose(&request(
            &ui,
            1,
            "alt_create",
            json!({"kind":"paragraph","start":0,"end":12,"text":"Other\nchoice"}),
        ))
        .unwrap();
    let mut host = HostSession::new("file:///a.md", "host", 1, "First\r\nbanana").unwrap();
    let p = host
        .propose(&request(
            &host,
            1,
            "replace_source",
            json!({"source":p["source"]}),
        ))
        .unwrap();
    assert_eq!(
        p["projection"]["annotations"]["spans"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let span = &p["projection"]["annotations"]["spans"][0];
    assert_eq!(span["id"], "s1");
    assert_eq!(span["active"], 0);
    assert_eq!(span["anchor"]["text"], "First\r\nbanana");
    assert_eq!(span["alts"][1]["text"], "Other\r\nchoice");
    assert_eq!(p["projection"]["notice"], Value::Null);
}
#[test]
fn crlf_conversion_includes_whole_newline_at_ghost_start() {
    let mut ui = HostSession::new("file:///a.md", "ui", 1, "First\nbanana").unwrap();
    let p = ui
        .propose(&request(&ui, 1, "ghost", json!({"start":5,"end":12})))
        .unwrap();
    let mut host = HostSession::new("file:///a.md", "host", 1, "First\r\nbanana").unwrap();
    let p = host
        .propose(&request(
            &host,
            1,
            "replace_source",
            json!({"source":p["source"]}),
        ))
        .unwrap();
    assert_eq!(
        p["projection"]["annotations"]["ghosts"][0]["anchor"]["text"],
        "\r\nbanana"
    );
    assert_eq!(p["projection"]["export"], "First");
}
#[test]
fn crlf_full_replacement_refuses_malformed_mixed_eol_instead_of_changing_raw_block() {
    let source = "Body\r\n\r\n```terraphim-alternatives\n{ broken\r\n```\n";
    let mut host = HostSession::new("file:///a.md", "host", 1, source).unwrap();
    let before = host.snapshot();
    assert!(host
        .propose(&request(
            &host,
            1,
            "edit",
            json!({"start":0,"deleted":4,"inserted":"Changed"})
        ))
        .is_err());
    assert_eq!(host.snapshot(), before);
}

#[test]
fn presentation_roundtrip_preserves_full_multiline_annotations_and_raw_malformed_suffix() {
    use terraphim_editor::host::host_presentation_source;
    let mut ui = HostSession::new("file:///a.md", "ui", 1, "First\nbanana").unwrap();
    let original = ui
        .propose(&request(
            &ui,
            1,
            "alt_create",
            json!({"kind":"paragraph","start":0,"end":12,"text":"Other\nchoice"}),
        ))
        .unwrap();
    let mut host = HostSession::new("file:///a.md", "host", 1, "First\r\nbanana").unwrap();
    let native = host
        .propose(&request(
            &host,
            1,
            "replace_source",
            json!({"source":original["source"]}),
        ))
        .unwrap();
    assert_eq!(
        host_presentation_source(native["source"].as_str().unwrap()).unwrap(),
        original["source"].as_str().unwrap()
    );
    let raw = "\n\n```terraphim-alternatives\r\n{ broken\n```\r\n";
    let source = format!("Body\r\nTail{raw}");
    let shown = host_presentation_source(&source).unwrap();
    assert!(shown.ends_with(raw));
    assert!(shown.starts_with("Body\nTail"));
}

#[test]
fn unresolved_multiline_annotations_keep_valid_lengths_in_both_eol_directions() {
    use terraphim_alternatives::{Alternative, Anchor, Document, Ghost, Source, Span, SpanKind};
    use terraphim_editor::host::host_presentation_source;
    let mut doc = Document::new("New\nbody");
    let anchor = Anchor {
        start: 100,
        end: 103,
        text: "A\nB".into(),
        before: None,
        after: None,
    };
    doc.annotations.spans.push(Span {
        id: "s1".into(),
        kind: SpanKind::Paragraph,
        anchor: anchor.clone(),
        active: 0,
        alts: vec![
            Alternative::new("A\nB", Source::Original),
            Alternative::new("C\nD", Source::Human),
        ],
    });
    doc.annotations.ghosts.push(Ghost {
        id: "g1".into(),
        anchor,
    });
    let source = terraphim_alternatives::write(&doc);
    let mut host = HostSession::new("file:///a.md", "host", 1, "New\r\nbody").unwrap();
    let p = host
        .propose(&request(
            &host,
            1,
            "replace_source",
            json!({"source":source}),
        ))
        .unwrap();
    assert_eq!(p["projection"]["annotations"]["preservedBlock"], false);
    let pending = &p["projection"]["annotations"]["setAside"];
    assert_eq!(pending["spans"][0]["anchor"]["text"], "A\r\nB");
    assert_eq!(
        pending["spans"][0]["anchor"]["end"].as_u64().unwrap()
            - pending["spans"][0]["anchor"]["start"].as_u64().unwrap(),
        4
    );
    assert_eq!(pending["ghosts"].as_array().unwrap().len(), 1);
    let lf = host_presentation_source(p["source"].as_str().unwrap()).unwrap();
    let parsed = terraphim_alternatives::parse(&lf).unwrap();
    assert_eq!(parsed.annotations.spans[0].anchor.text, "A\nB");
    assert_eq!(
        parsed.annotations.spans[0].anchor.end - parsed.annotations.spans[0].anchor.start,
        3
    );
}
#[test]
fn presentation_refuses_annotation_boundary_splitting_crlf() {
    use terraphim_editor::host::host_presentation_source;
    let mut ui = HostSession::new("file:///a.md", "ui", 1, "A\r\nB").unwrap();
    let p = ui.propose(&request(&ui, 1, "ghost", json!({"start":1,"end":2})));
    assert!(
        p.is_err(),
        "normalization must refuse a half-CRLF annotation atomically"
    );
    let mut doc = terraphim_alternatives::Document::new("A\r\nB");
    doc.ghost(1, 2).unwrap();
    assert!(host_presentation_source(&terraphim_alternatives::write(&doc)).is_err());
}
