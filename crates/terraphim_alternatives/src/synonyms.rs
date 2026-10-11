//! Document-scoped KG registration using Terraphim's directive parser and types.
//!
//! An authored span is a concept in its own thesaurus. This prevents identical
//! phrases in unrelated spans from overwriting one another. The annotation
//! block is authoritative; rebuilding after edits/history cannot retain stale
//! aliases. No shared role or global graph is modified.

use terraphim_automata::{ConceptIndex, parse_markdown_directives_str};
use terraphim_types::{NormalizedTerm, NormalizedTermValue, Thesaurus};

use crate::{Alternative, Document, Span};

/// One span registered as a KG concept with its exact ordered provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynonymGroup {
    /// Identity within the owning document; never a global concept identity.
    pub span_id: String,
    /// Standard KG Markdown, consumed by the existing directive parser.
    pub markdown: String,
    /// Exact terms that the comma/line-delimited directive cannot represent.
    /// These are registered through `NormalizedTermValue`, without splitting.
    pub typed_synonyms: Vec<String>,
    /// Actual Terraphim synonym-to-concept map, isolated to this span.
    pub thesaurus: Thesaurus,
    /// Existing reverse concept-to-synonyms index over the registered map.
    pub index: ConceptIndex,
    /// Exact display text, order, source and model, including case duplicates.
    pub alternatives: Vec<Alternative>,
}

impl SynonymGroup {
    /// Register a span with the real directive parser, types and concept index.
    /// Empty normalized text remains in provenance but has no searchable key.
    pub fn from_span(span: &Span) -> Self {
        let (directives, typed_synonyms): (Vec<_>, Vec<_>) = span
            .alts
            .iter()
            .map(|alt| alt.text.clone())
            .partition(|text| !text.contains([',', '\r', '\n']));
        let markdown = format!("synonyms:: {}\n", directives.join(", "));
        let parsed = parse_markdown_directives_str("alternative", &markdown);
        let mut thesaurus = Thesaurus::new(format!("alternative:{}", span.id));
        // Concept IDs are local to this thesaurus. The caller also carries the
        // resource and span ID; merging these maps would erase that scope.
        let canonical_alternative = span.alts.iter().find(|alt| !alt.text.trim().is_empty());
        let canonical = canonical_alternative
            .map(|alt| NormalizedTermValue::from(alt.text.as_str()))
            .unwrap_or_default();
        let mut concept = NormalizedTerm::new(1, canonical);
        concept.display_value = canonical_alternative.map(|alt| alt.text.clone());
        for text in parsed.directives["alternative"]
            .synonyms
            .iter()
            .chain(typed_synonyms.iter())
        {
            let key = NormalizedTermValue::from(text.as_str());
            if !key.as_str().is_empty() {
                thesaurus.insert(key, concept.clone());
            }
        }
        let index = ConceptIndex::from_thesaurus(&thesaurus);
        Self {
            span_id: span.id.clone(),
            markdown,
            typed_synonyms,
            thesaurus,
            index,
            alternatives: span.alts.clone(),
        }
    }

    /// Look up an authored synonym through the actual registered concept index.
    /// Unknown or blank terms return an empty slice.
    pub fn synonyms_of(&self, text: &str) -> &[NormalizedTermValue] {
        self.thesaurus
            .get(&NormalizedTermValue::from(text))
            .map_or(&[], |term| self.index.synonyms_of(term.id))
    }
}

impl Document {
    /// Register current authored alternatives as isolated KG synonym concepts.
    /// Re-deriving from current annotations makes save/reopen and undo/redo
    /// restore membership without a separately authoritative graph cache.
    pub fn synonym_groups(&self) -> Vec<SynonymGroup> {
        self.annotations
            .spans
            .iter()
            .filter(|span| !span.is_inert())
            .map(SynonymGroup::from_span)
            .collect()
    }
}

/// A document group cannot be represented without changing its meaning.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SynonymExportError {
    /// The shared flat thesaurus cannot distinguish the two scoped concepts.
    #[error(
        "Cannot share the graph: '{term}' belongs to different alternative groups. Rename an alternative or remove the conflicting group first."
    )]
    AmbiguousTerm {
        /// Normalized term shared by multiple distinct document groups.
        term: String,
    },
}

impl Document {
    /// Export current attached synonym groups as a portable Terraphim thesaurus.
    ///
    /// Uses the same parsed `synonyms::` and typed registrations as the editor.
    /// Each span keeps a distinct exported concept ID. The document and active
    /// role are unchanged; the returned graph is an explicit sharing snapshot.
    ///
    /// # Errors
    /// Refuses terms present in multiple groups instead of merging their meanings.
    pub fn export_synonym_thesaurus(&self) -> Result<Thesaurus, SynonymExportError> {
        let mut graph = Thesaurus::new("Document alternatives".into());
        let mut attached = self.clone();
        attached.reanchor();
        for (offset, group) in attached.synonym_groups().iter().enumerate() {
            for (key, value) in &group.thesaurus {
                if graph.get(key).is_some() {
                    return Err(SynonymExportError::AmbiguousTerm {
                        term: key.to_string(),
                    });
                }
                let mut concept = value.clone();
                concept.id = offset as u64 + 1;
                graph.insert(key.clone(), concept);
            }
        }
        Ok(graph)
    }
}
