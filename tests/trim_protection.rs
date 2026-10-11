use terraphim_editor::protection::ConceptProtection;
use terraphim_lab::{
    automata::{MatchOverlap, MatcherBuilder, MatcherOptions},
    make_cuts, trim_plan, trim_plan_excluding,
    types::Thesaurus,
    Folded, LabConfig, TrimLevel,
};

fn role(data: &str) -> Thesaurus {
    serde_json::from_str(&format!(r#"{{"name":"public-test","data":{data}}}"#)).unwrap()
}

#[test]
fn actual_upstream_all_min_one_differential_and_original_offsets() {
    let role = role(
        r#"{"i":{"id":1,"nterm":"i"},"x":{"id":2,"nterm":"x"},"rust":{"id":3,"nterm":"rust"},"rust async":{"id":3,"nterm":"rust"},"async":{"id":4,"nterm":"async"},"alias":{"id":3,"nterm":"rust"}}"#,
    );
    let protection = ConceptProtection::from_thesaurus(&role).unwrap();
    let body = "I x Rust\r\n async alias rust async mix";
    let folded = Folded::new(body);
    let mut builder = MatcherBuilder::new(
        MatcherOptions::default()
            .with_overlap(MatchOverlap::All)
            .with_min_pattern_length(1)
            .with_word_boundaries(true),
    );
    for (key, term) in &role {
        builder
            .insert(key.as_str().to_owned(), term.clone())
            .unwrap();
    }
    let matcher = builder.build().unwrap();
    let mut expected: Vec<_> = matcher
        .find_positions(&folded.text)
        .map(|m| {
            let (start, end) = folded.to_original(m.start, m.end);
            (matcher.term(m.pattern_index).unwrap().id, start, end)
        })
        .collect();
    expected.sort_unstable();
    let mut actual: Vec<_> = protection
        .occurrences(body)
        .unwrap()
        .iter()
        .map(|o| (o.concept_id, o.range.start, o.range.end))
        .collect();
    actual.sort_unstable();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 9);
    assert!(actual
        .iter()
        .any(|&(_, s, e)| &body[s..e] == "Rust\r\n async"));
    let gaps = "rust\t async rust\u{a0}async";
    let hits = protection.occurrences(gaps).unwrap();
    assert_eq!(hits.len(), 6);
    assert!(hits
        .iter()
        .any(|o| &gaps[o.range.start..o.range.end] == "rust\t async"));
    assert!(hits
        .iter()
        .any(|o| &gaps[o.range.start..o.range.end] == "rust\u{a0}async"));
}

#[test]
fn exact_unicode_and_whole_boundaries() {
    let protection = ConceptProtection::from_thesaurus(&role(r#"{"中":{"id":1,"nterm":"中"},"α":{"id":2,"nterm":"α"},"я":{"id":3,"nterm":"я"},"x":{"id":4,"nterm":"x"}}"#)).unwrap();
    let body = "中 α я X 中文 αβ яа ax x-ray x/y";
    let hits = protection.occurrences(body).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| &body[h.range.start..h.range.end])
            .collect::<Vec<_>>(),
        ["中", "α", "я", "X"]
    );
    let large_id =
        ConceptProtection::from_thesaurus(&role(r#"{"x":{"id":9007199254740993,"nterm":"x"}}"#))
            .unwrap();
    assert_eq!(
        large_id.occurrences("X x").unwrap()[1].concept_id,
        9_007_199_254_740_993
    );
}

#[test]
fn unicode_documents_are_supported_and_empty_roles_still_refused() {
    let p =
        ConceptProtection::from_thesaurus(&role(r#"{"cafe":{"id":1,"nterm":"cafe"}}"#)).unwrap();
    for body in [
        "cafe\u{301}",
        "Straße",
        "STRASSE Straße",
        "Α",
        "Я",
        "\u{1100}\u{1161}",
    ] {
        assert!(p.occurrences(body).is_ok(), "{body}");
    }
    assert!(ConceptProtection::from_thesaurus(&role("{}"))
        .unwrap_err()
        .contains("role_required"));
}

#[test]
fn protection_filters_before_fit_and_retains_every_occurrence_all_levels() {
    let body = "The editor, which owns its DOM, is basically the only target here today. It is basically quite useful.\n\nBasically, the editor works.";
    let config = LabConfig::with_defaults().unwrap();
    assert_eq!(
        trim_plan(body, &config),
        trim_plan_excluding(body, &config, &[])
    );
    let protection = ConceptProtection::from_thesaurus(&role(r#"{"basically":{"id":1,"nterm":"basically"},"owns its dom":{"id":2,"nterm":"owns its dom"}}"#)).unwrap();
    let inventory = protection.occurrences(body).unwrap();
    assert_eq!(inventory.len(), 4);
    let plan = protection.plan(body, &config).unwrap();
    for level in TrimLevel::ALL {
        let active = plan.active(level, &[]);
        protection.validate_ranges(body, &active).unwrap();
        let made = make_cuts(body, &active);
        protection.validate_retention(body, &made).unwrap();
        for kept in plan.cuts() {
            let active = plan.active(level, &[kept.id]);
            protection.validate_ranges(body, &active).unwrap();
            protection
                .validate_retention(body, &make_cuts(body, &active))
                .unwrap();
        }
    }
}

#[test]
fn tidy_capitalization_and_duplicate_loss_refused() {
    let body = "Perhaps, rust is useful. Rust is useful.";
    let protection =
        ConceptProtection::from_thesaurus(&role(r#"{"rust":{"id":1,"nterm":"rust"}}"#)).unwrap();
    let made = terraphim_lab::make_cuts_from_ranges(body, [(0, 9)]);
    assert!(made.text.starts_with("Rust"));
    assert!(protection.validate_retention(body, &made).is_err());
    let made = terraphim_lab::make_cuts_from_ranges(body, [(0, 24)]);
    assert!(protection.validate_retention(body, &made).is_err());
    let join = "rust and useful";
    let made = terraphim_lab::make_cuts_from_ranges(join, [(4, 9)]);
    assert_eq!(made.text, "rustuseful");
    assert!(protection.validate_retention(join, &made).is_err());
}

#[test]
fn unicode_byte_ranges_survive_actual_utf16_edits_and_fold_collisions_refuse() {
    let protection = ConceptProtection::from_thesaurus(&role(
        r#"{"α":{"id":1,"nterm":"α"},"я":{"id":2,"nterm":"я"},"中":{"id":3,"nterm":"中"}}"#,
    ))
    .unwrap();
    let body = "α works (some aside) and я and 中.";
    let start_byte = body.find(" (some").unwrap();
    let end_byte = body.find(')').unwrap() + 1;
    let start = body[..start_byte].encode_utf16().count();
    let end = body[..end_byte].encode_utf16().count();
    let made = terraphim_lab::make_cuts_from_ranges(body, [(start, end)]);
    assert_eq!(made.text, "α works and я and 中.");
    protection.validate_retention(body, &made).unwrap();
    let hits = protection.occurrences(&made.text).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| (h.range.start, h.range.end))
            .collect::<Vec<_>>(),
        [(0, 2), (13, 15), (20, 23)]
    );
    let colliding =
        role(r#"{"rust async":{"id":1,"nterm":"rust"},"rust\nasync":{"id":2,"nterm":"other"}}"#);
    assert!(ConceptProtection::from_thesaurus(&colliding)
        .unwrap_err()
        .contains("invalid_protection_role"));
}
