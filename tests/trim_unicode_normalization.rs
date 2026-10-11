use terraphim_editor::protection::ConceptProtection;
use terraphim_lab::{make_cuts, types::Thesaurus, LabConfig, TrimLevel};

fn protection(keys: &[&str]) -> ConceptProtection {
    let data: serde_json::Map<String, serde_json::Value> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            (
                key.to_string(),
                serde_json::json!({"id": i + 1, "nterm": key}),
            )
        })
        .collect();
    let role: Thesaurus =
        serde_json::from_value(serde_json::json!({"name":"unicode", "data":data})).unwrap();
    ConceptProtection::from_thesaurus(&role).unwrap()
}

#[test]
fn canonical_case_and_original_grapheme_ranges() {
    let p = protection(&["café", "straße", "α", "я", "가", "i\u{307}"]);
    let body = "🧠 Café cafe\u{301} STRAẞE Straße Α Я \u{1100}\u{1161} İ";
    let hits = p.occurrences(body).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| (&body[h.range.start..h.range.end], h.concept_id))
            .collect::<Vec<_>>(),
        [
            ("Café", 1),
            ("cafe\u{301}", 1),
            ("STRAẞE", 2),
            ("Straße", 2),
            ("Α", 3),
            ("Я", 4),
            ("\u{1100}\u{1161}", 5),
            ("İ", 6)
        ]
    );
}

#[test]
fn whitespace_original_ranges_and_partial_graphemes() {
    let p = protection(&["café α", "i", "cafe"]);
    let body = "🧠 cafe\u{301}\r\n\tΑ İ cafe\u{301}";
    let hits = p.occurrences(body).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        &body[hits[0].range.start..hits[0].range.end],
        "cafe\u{301}\r\n\tΑ"
    );
    // Terraphim lowercase normalization is not German transliteration.
    assert!(protection(&["straße"])
        .occurrences("STRASSE")
        .unwrap()
        .is_empty());
}

#[test]
fn actual_trim_preserves_unicode_at_every_level() {
    let p = protection(&["café", "straße", "α", "я"]);
    let body = "🧠 Café cafe\u{301} STRAẞE Α Я are basically quite useful. The editor is basically the only target here today.";
    let config = LabConfig::with_defaults().unwrap();
    let plan = p.plan(body, &config).unwrap();
    let before = p.occurrences(body).unwrap();
    assert_eq!(before.len(), 5);
    let mut reduced = false;
    for level in TrimLevel::ALL {
        let active = plan.active(level, &[]);
        p.validate_ranges(body, &active).unwrap();
        let made = make_cuts(body, &active);
        p.validate_retention(body, &made).unwrap();
        assert_eq!(p.occurrences(&made.text).unwrap().len(), before.len());
        reduced |= made.text.len() < body.len();
    }
    assert!(
        reduced,
        "must prove actual reduction, not only an unchanged input"
    );
}

#[test]
fn canonical_role_collisions_remain_rejected() {
    let role: Thesaurus = serde_json::from_value(serde_json::json!({"name":"collision", "data":{
        "café":{"id":1,"nterm":"a"},"cafe\u{301}":{"id":2,"nterm":"b"}
    }}))
    .unwrap();
    assert!(ConceptProtection::from_thesaurus(&role).is_err());
}

#[test]
fn contextual_lowercase_and_complete_emoji_clusters() {
    let p = protection(&["ος", "οσα", "👩", "👩‍💻"]);
    let body = "ΟΣ ΟΣΑ 👩‍💻";
    let hits = p.occurrences(body).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| (&body[h.range.start..h.range.end], h.concept_id))
            .collect::<Vec<_>>(),
        [("ΟΣ", 1), ("ΟΣΑ", 2), ("👩‍💻", 4)]
    );
}

#[test]
fn unicode_context_crosses_editor_word_boundaries() {
    for (key, body) in [
        ("οσ·α", "ΟΣ·Α"),
        ("οσ''α", "ΟΣ''Α"),
        ("οσ:α", "ΟΣ:Α"),
        ("ος", "ΟΣ"),
    ] {
        let p = protection(&[key]);
        let hits = p.occurrences(body).unwrap();
        assert_eq!(hits.len(), 1, "{key} / {body}");
        assert_eq!(&body[hits[0].range.start..hits[0].range.end], body);
    }
}
