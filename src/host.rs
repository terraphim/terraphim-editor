//! Instance-owned host protocol over the existing document engine.
//!
//! The host owns text revisions and history. Proposals are staged on clones;
//! only exact acknowledged output commits. This module has no DOM calls.
use crate::document::{counts_json, parse_span_kind, DocumentSession};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use terraphim_alternatives::Source;

/// Maximum source or message size, in UTF-8 bytes.
pub const HOST_SOURCE_LIMIT: usize = 16 * 1024 * 1024;
const MAX_REQUEST_ID: u64 = 9_007_199_254_740_991;

/// SHA-256 of exact serialized source bytes, including annotations and CRLF.
pub fn source_hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

#[derive(Debug)]
struct Pending {
    id: u64,
    source: String,
    document: DocumentSession,
}

/// One resource/session, independent of the legacy browser thread-local API.
#[derive(Debug)]
pub struct HostSession {
    resource: String,
    session: String,
    revision: u32,
    source: String,
    document: DocumentSession,
    last_request: u64,
    pending: Option<Pending>,
}
impl HostSession {
    /// Open exact host source. Malformed annotation blocks remain preserved.
    ///
    /// # Errors
    /// Empty identities or oversized source are refused.
    pub fn new(resource: &str, session: &str, revision: u32, source: &str) -> Result<Self, String> {
        if resource.is_empty() || session.is_empty() {
            return Err("empty host identity".into());
        }
        bounded(source)?;
        let mut document = DocumentSession::new();
        document.open(source);
        Ok(Self {
            resource: resource.into(),
            session: session.into(),
            revision,
            source: source.into(),
            document,
            last_request: 0,
            pending: None,
        })
    }
    /// Exact current source and host-neutral render projection.
    pub fn snapshot(&self) -> Value {
        json!({"protocol":1,"resource":self.resource,"session":self.session,
            "revision":self.revision,"sourceHash":source_hash(&self.source),
            "source":self.source,"projection":projection(&self.document)})
    }
    /// Stage one domain command against an exact host revision/source hash.
    ///
    /// # Errors
    /// Stale/malformed envelopes, repeated IDs, pending operations and invalid
    /// domain commands are refused without changing the committed document.
    pub fn propose(&mut self, message: &str) -> Result<Value, String> {
        bounded(message)?;
        let request: Value = serde_json::from_str(message).map_err(|e| e.to_string())?;
        if request["protocol"] != 1
            || request["resource"] != self.resource
            || request["session"] != self.session
            || request["revision"] != self.revision
            || request["sourceHash"] != source_hash(&self.source)
        {
            return Err("stale or incompatible host envelope".into());
        }
        let id = number(&request, "id")?;
        if id == 0 || id > MAX_REQUEST_ID || id <= self.last_request || self.pending.is_some() {
            return Err("duplicate request or operation pending".into());
        }
        let mut document = self.document.clone();
        execute(
            &mut document,
            string(&request, "command")?,
            &request["args"],
        )?;
        let source = if self.source.contains("\r\n") {
            source_eol(&document.save(), true)?
        } else {
            document.save()
        };
        document.open(&source);
        bounded(&source)?;
        self.last_request = id;
        let response = json!({"protocol":1,"resource":self.resource,"session":self.session,
            "id":id,"revision":self.revision,"beforeHash":source_hash(&self.source),
            "sourceHash":source_hash(&source),"source":source,"projection":projection(&document)});
        self.pending = Some(Pending {
            id,
            source,
            document,
        });
        Ok(response)
    }
    /// Commit only the exact actual source reported by the host after applying.
    ///
    /// # Errors
    /// Wrong ID, non-advancing revision or nonmatching output leaves the staged
    /// operation available for explicit rejection or a correct acknowledgement.
    pub fn acknowledge(&mut self, id: u64, revision: u32, source: &str) -> Result<Value, String> {
        let pending = self.pending.as_ref().ok_or("no pending proposal")?;
        if pending.id != id || revision <= self.revision || pending.source != source {
            return Err("host acknowledgement does not match staged output".into());
        }
        let pending = self.pending.take().ok_or("no pending proposal")?;
        self.document = pending.document;
        self.source = pending.source;
        self.revision = revision;
        Ok(self.snapshot())
    }
    /// Cancel the named proposal. Its ID remains consumed to prevent replay.
    ///
    /// # Errors
    /// A missing or different pending operation is refused.
    pub fn reject(&mut self, id: u64) -> Result<(), String> {
        if !self.pending.as_ref().is_some_and(|p| p.id == id) {
            return Err("unknown proposal".into());
        }
        self.pending = None;
        Ok(())
    }
    /// Adopt an actual external edit or native undo/redo; invalidate proposals.
    ///
    /// # Errors
    /// Oversized source and non-advancing revisions are refused.
    pub fn synchronize(&mut self, revision: u32, source: &str) -> Result<Value, String> {
        bounded(source)?;
        if revision <= self.revision {
            return Err("host revision must advance".into());
        }
        let mut document = DocumentSession::new();
        document.open(source);
        self.document = document;
        self.source = source.into();
        self.revision = revision;
        self.pending = None;
        Ok(self.snapshot())
    }
}
// EOL changes transform coordinates and annotation text together. They are
// not semantic insertions, which would detach multiline spans or split ghosts.
fn source_eol(source: &str, crlf: bool) -> Result<String, String> {
    let normalize = |text: &str| {
        let lf = text.replace("\r\n", "\n").replace('\r', "\n");
        if crlf {
            lf.replace('\n', "\r\n")
        } else {
            lf
        }
    };
    let mut document = match terraphim_alternatives::parse(source) {
        Ok(document) => document,
        Err(error) => {
            if crlf && normalize(&error.raw_block) != error.raw_block {
                return Err("full replacement would alter a preserved malformed block's line endings; repair it in the native text editor".into());
            }
            return Ok(normalize(&error.body) + &error.raw_block);
        }
    };
    let mut changes = Vec::new();
    let mut split_boundaries = Vec::new();
    let mut offset = 0;
    let mut chars = document.body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' && chars.peek() == Some(&'\n') {
            split_boundaries.push(offset + 1);
            if !crlf {
                changes.push(offset);
            }
            chars.next();
            offset += 2;
        } else {
            if crlf && (ch == '\n' || ch == '\r') {
                changes.push(offset);
            }
            offset += ch.len_utf16();
        }
    }
    let map = |at: usize| {
        let delta = changes.partition_point(|&position| position < at);
        if crlf {
            at.checked_add(delta)
        } else {
            at.checked_sub(delta)
        }
        .ok_or_else(|| "EOL anchor offset overflow".to_string())
    };
    let anchor = |value: &mut terraphim_alternatives::Anchor| -> Result<(), String> {
        if split_boundaries.binary_search(&value.start).is_ok()
            || split_boundaries.binary_search(&value.end).is_ok()
        {
            return Err("annotation boundary splits a CRLF newline".into());
        }
        value.start = map(value.start)?;
        value.text = normalize(&value.text);
        // Unresolved anchors may describe text with different line endings
        // from the body at their stale hint. Keep their own normalized extent.
        value.end = value
            .start
            .checked_add(terraphim_alternatives::utf16_len(&value.text))
            .ok_or_else(|| "EOL anchor extent overflow".to_string())?;
        value.before = value.before.as_deref().map(normalize);
        value.after = value.after.as_deref().map(normalize);
        Ok(())
    };
    for span in &mut document.annotations.spans {
        anchor(&mut span.anchor)?;
        for alternative in &mut span.alts {
            alternative.text = normalize(&alternative.text);
        }
    }
    for ghost in &mut document.annotations.ghosts {
        anchor(&mut ghost.anchor)?;
    }
    document.body = normalize(&document.body);
    document.refresh_context();
    let output = normalize(&terraphim_alternatives::write(&document));
    terraphim_alternatives::parse(&output)
        .map_err(|e| format!("invalid EOL annotation transformation: {e}"))?;
    Ok(output)
}

/// LF source for a DOM presentation, with rebased annotations and untouched
/// malformed block bytes. Native persisted source remains owned by its host.
///
/// # Errors
/// Refuses overflowing annotation coordinates.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn host_presentation_source(source: &str) -> Result<String, String> {
    bounded(source)?;
    source_eol(source, false)
}
fn bounded(source: &str) -> Result<(), String> {
    if source.len() > HOST_SOURCE_LIMIT {
        Err("host source/message exceeds size limit".into())
    } else {
        Ok(())
    }
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))
}
fn number(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("{key} must be a nonnegative integer"))
}
fn offset(value: &Value, key: &str) -> Result<usize, String> {
    usize::try_from(number(value, key)?).map_err(|_| format!("{key} exceeds offset range"))
}
fn projection(document: &DocumentSession) -> Value {
    json!({"body":document.body(),"annotations":document.annotations_json(),
        "counts":counts_json(document.counts()),"overflow":document.overflow(),
        "export":document.export(),"notice":document.set_aside_notice()})
}
fn execute(document: &mut DocumentSession, command: &str, args: &Value) -> Result<(), String> {
    if document.annotations_json()["preservedBlock"] == true
        && !matches!(command, "edit" | "replace_source")
    {
        return Err("cannot change annotations while a malformed block is preserved".into());
    }
    match command {
        "edit" => {
            document
                .apply_edit(
                    offset(args, "start")?,
                    offset(args, "deleted")?,
                    string(args, "inserted")?,
                )
                .map_err(|e| e.to_string())?;
        }
        "ghost" => {
            document
                .ghost(offset(args, "start")?, offset(args, "end")?)
                .map_err(|e| e.to_string())?;
        }
        "revive" => {
            document
                .revive(offset(args, "start")?, offset(args, "end")?)
                .map_err(|e| e.to_string())?;
        }
        "alt_create" => {
            document.alt_create_span(
                parse_span_kind(string(args, "kind")?)?,
                offset(args, "start")?,
                offset(args, "end")?,
                string(args, "text")?,
                Source::Human,
            )?;
        }
        "alt_add" => {
            document.alt_add(string(args, "span")?, string(args, "text")?, Source::Human)?;
        }
        "alt_edit" => {
            document.alt_edit(
                string(args, "span")?,
                offset(args, "index")?,
                string(args, "text")?,
            )?;
        }
        "alt_remove" => {
            document.alt_remove(string(args, "span")?, offset(args, "index")?)?;
        }
        "alt_move" => {
            document.alt_move(
                string(args, "span")?,
                offset(args, "from")?,
                offset(args, "to")?,
            )?;
        }
        "alt_active" => {
            document
                .set_active(string(args, "span")?, offset(args, "index")?)
                .map_err(|e| e.to_string())?;
        }
        "overflow_set" => {
            document
                .set_overflow(string(args, "text")?)
                .map_err(|e| e.to_string())?;
        }
        "stash" => {
            document
                .stash(offset(args, "start")?, offset(args, "end")?)
                .map_err(|e| e.to_string())?;
        }
        "pull" => {
            let pulled = document
                .pull_back_text(
                    offset(args, "at")?,
                    offset(args, "from")?,
                    offset(args, "to")?,
                )
                .map_err(|e| e.to_string())?;
            document
                .apply_edit(pulled.start, 0, &pulled.text)
                .map_err(|e| e.to_string())?;
        }
        // Browser presentation already uses the same domain engine. Its exact
        // saved source can cross the host boundary as one native undo step.
        "replace_source" => {
            document.open(string(args, "source")?);
        }
        _ => return Err(format!("unsupported host command: {command}")),
    }
    Ok(())
}

/// Browser binding with one owned session per instance; no DOM dependency.
#[wasm_bindgen::prelude::wasm_bindgen(js_name = TerraphimHostSession)]
pub struct WasmHostSession {
    inner: HostSession,
}

#[wasm_bindgen::prelude::wasm_bindgen(js_class = TerraphimHostSession)]
impl WasmHostSession {
    /// Open a host resource with exact source and revision.
    #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
    pub fn new(resource: &str, session: &str, revision: u32, source: &str) -> Result<Self, String> {
        Ok(Self {
            inner: HostSession::new(resource, session, revision, source)?,
        })
    }
    /// Current source and render projection, encoded as protocol JSON.
    pub fn snapshot(&self) -> String {
        self.inner.snapshot().to_string()
    }
    /// Stage a protocol command; committed source stays unchanged.
    pub fn propose(&mut self, message: &str) -> Result<String, String> {
        self.inner.propose(message).map(|value| value.to_string())
    }
    /// Commit exact host output after the native text transaction.
    pub fn acknowledge(&mut self, id: f64, revision: u32, source: &str) -> Result<String, String> {
        self.inner
            .acknowledge(js_id(id)?, revision, source)
            .map(|value| value.to_string())
    }
    /// Invalidate a named staged operation.
    pub fn reject(&mut self, id: f64) -> Result<(), String> {
        self.inner.reject(js_id(id)?)
    }
    /// Adopt actual external edits or native undo/redo.
    pub fn synchronize(&mut self, revision: u32, source: &str) -> Result<String, String> {
        self.inner
            .synchronize(revision, source)
            .map(|value| value.to_string())
    }
}
fn js_id(id: f64) -> Result<u64, String> {
    if !id.is_finite() || id.fract() != 0.0 || id < 1.0 || id > MAX_REQUEST_ID as f64 {
        return Err("request ID must be a positive safe integer".into());
    }
    Ok(id as u64)
}
