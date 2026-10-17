//! Native adapter contracts and real WASM/panel targeting. Browser execution
//! is intentionally delegated to the parent's serial, load-guarded run.
use terraphim_editor::{text_range_json, TextTargetError};

#[cfg(target_arch = "wasm32")]
mod support;

#[test]
fn utf16_caret_floors_only_for_targeting_and_errors_are_typed() {
    assert_eq!(
        text_range_json("𐐀x cafe\u{301}", 1, "word").unwrap(),
        serde_json::json!({"start":0,"end":3})
    );
    assert_eq!(terraphim_alternatives::utf16_to_byte("𐐀x", 1), None);
    assert_eq!(
        text_range_json("x", 2, "word"),
        Err(TextTargetError::OffsetOutOfRange)
    );
    assert_eq!(
        text_range_json("x", 0, "unknown"),
        Err(TextTargetError::UnknownKind("unknown".into()))
    );
    assert_eq!(
        text_range_json("", 0, "word").unwrap(),
        serde_json::Value::Null
    );
}

#[test]
fn markdown_heading_body_without_gap_crlf_and_fences_have_exact_scopes() {
    let text = "# Head line\r\nBody. Next.\r\n\r\n```\r\nnot prose. Next.\r\n```";
    assert_eq!(
        text_range_json(text, 3, "sentence").unwrap(),
        serde_json::json!({"start":2,"end":11})
    );
    assert_eq!(
        text_range_json(text, 15, "paragraph").unwrap(),
        serde_json::json!({"start":13,"end":24})
    );
    assert_eq!(
        text_range_json(text, 35, "sentence").unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        text_range_json(text, 35, "word").unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        text_range_json("Title\n=====\nBody.", 2, "sentence").unwrap(),
        serde_json::json!({"start":0,"end":5})
    );
}

#[test]
fn adapter_retains_combining_words_repeated_offsets_and_inline_protection() {
    let text = "cafe\u{301}x. Same. Same.";
    assert_eq!(
        text_range_json(text, 5, "word").unwrap(),
        serde_json::json!({"start":0,"end":6})
    );
    assert_eq!(
        text_range_json(text, 16, "sentence").unwrap(),
        serde_json::json!({"start":14,"end":19})
    );
    let text = "Use `code. Here` now. `lower` next.";
    assert_eq!(
        text_range_json(text, 12, "sentence").unwrap(),
        serde_json::json!({"start":0,"end":21})
    );
    assert_eq!(
        text_range_json(text, 25, "sentence").unwrap(),
        serde_json::json!({"start":22,"end":35})
    );
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::support::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn actual_wasm_binding_returns_utf16_null_and_refusals() {
        let value = terraphim_editor::text_range_at("𐐀x", 1, "word").unwrap();
        assert_eq!(
            js_sys::JSON::stringify(&value)
                .unwrap()
                .as_string()
                .unwrap(),
            "{\"end\":3,\"start\":0}"
        );
        assert!(terraphim_editor::text_range_at("", 0, "word")
            .unwrap()
            .is_null());
        assert!(terraphim_editor::text_range_at("x", 2, "word").is_err());
        assert!(terraphim_editor::text_range_at("x", 0, "unknown").is_err());
        assert_eq!(terraphim_editor::text_infer_kind("cafe\u{301}x"), "word");
        assert_eq!(
            terraphim_editor::text_infer_kind("First. `lower` next."),
            "paragraph"
        );
    }

    // These are actual exported Rust functions, not a boundary mock. Existing
    // support installs its older export list, so install new exports explicitly.
    fn install_text_bindings() {
        let range = Closure::wrap(Box::new(|text: String, pos: u32, kind: String| {
            terraphim_editor::text_range_at(&text, pos, &kind)
        })
            as Box<dyn Fn(String, u32, String) -> Result<JsValue, JsValue>>);
        install_bindings("text_range_at", range.as_ref());
        range.forget();
        let infer = Closure::wrap(
            Box::new(|text: String| terraphim_editor::text_infer_kind(&text))
                as Box<dyn Fn(String) -> String>,
        );
        install_bindings("text_infer_kind", infer.as_ref());
        infer.forget();
    }

    #[wasm_bindgen_test]
    async fn real_panel_targets_unicode_and_markdown_through_wasm() {
        install_text_bindings();
        let _document = fresh_full_editor();
        sleep(0).await;
        install_alt_panel_helpers();
        let result = run_steps(&[
            r##"
              const body = '𐐀x cafe\u0301x. Same. Same.';
              s.setText(body);
              for (const pos of [0,1,2,3]) {
                s.setSelectionOffsets(pos,pos);
                ed.altPanel.open(undefined,{focus:false});
                ed.altPanel.setTab('word');
                if (ed.altPanel.target.pending.text !== '𐐀x') return 'astral '+pos;
              }
              for (let pos=4;pos<=10;pos++) {
                s.setSelectionOffsets(pos,pos);
                ed.altPanel.close({restoreFocus:false});
                ed.altPanel.open(undefined,{focus:false});
                if (ed.altPanel.target.pending.text !== 'cafe\u0301x') return 'combining '+pos;
              }
              s.setSelectionOffsets(20,20);
              ed.altPanel.close({restoreFocus:false});
              ed.altPanel.open(undefined,{focus:false});
              ed.altPanel.setTab('sentence');
              if (ed.altPanel.target.pending.start !== 18 || ed.altPanel.target.pending.text !== 'Same.') return 'repeat offset';
            "##,
            r##"
              s.setText('# Head line\r\nBody. Next.\r\n\r\n```\r\nnot prose. Next.\r\n```');
              s.setSelectionOffsets(3,3);
              ed.altPanel.close({restoreFocus:false});
              ed.altPanel.open(undefined,{focus:false});
              ed.altPanel.setTab('sentence');
              if (ed.altPanel.target.pending.text !== 'Head line') return 'heading';
              s.setSelectionOffsets(35,35);
              ed.altPanel.close({restoreFocus:false});
              ed.altPanel.open(undefined,{focus:false});
              if (!ed.altPanel.target.message || ed.altPanel.target.pending) return 'code targeted';
              ed.altPanel.open({start:35,end:38,kind:'word'},{focus:false});
              if (!ed.altPanel.target.pending) return 'explicit selection refused';
            "##,
        ]).await;
        assert_eq!(result, "");
    }

    #[wasm_bindgen_test]
    async fn missing_text_bindings_are_actionable_without_a_parser_fallback() {
        install_text_bindings();
        let _document = fresh_full_editor();
        sleep(0).await;
        install_alt_panel_helpers();
        let result = run_steps(&[r##"
          s.setText('Word. Next.'); s.setSelectionOffsets(1,1);
          const api = ed.documentApi();
          const old = api.text_range_at;
          delete api.text_range_at;
          try {
            ed.altPanel.open(undefined,{focus:false});
            if (ed.altPanel.target.pending || !/WASM.*reload/i.test(ed.altPanel.target.message || '')) return 'silent fallback';
          } finally { api.text_range_at = old; }
          ed.altPanel.open({start:0,end:4,kind:'word'},{focus:false});
          if (!ed.altPanel.target.pending) return 'explicit range unavailable';
        "##]).await;
        assert_eq!(result, "");
    }
}
