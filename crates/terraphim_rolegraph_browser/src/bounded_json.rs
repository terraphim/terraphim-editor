//! Streaming collection preflight, before allocating typed vectors/maps.
use crate::wire::*;
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use std::fmt;

const MARKER: &str = "bridge collection resource limit";
#[derive(Default)]
struct Budget {
    entries: usize,
    tags: usize,
    nodes: usize,
    matched: usize,
    adjacency: usize,
}
struct Seed<'a> {
    budget: &'a mut Budget,
    field: Option<String>,
}
impl Seed<'_> {
    fn item<E: Error>(&mut self, index: usize, is_map: bool) -> std::result::Result<(), E> {
        self.budget.entries += 1;
        let (cap, cumulative) = match self.field.as_deref() {
            Some("tags") if !is_map => (TAGS, Some((&mut self.budget.tags, 10_000))),
            Some("nodes") if !is_map => (NODES, Some((&mut self.budget.nodes, 100_000))),
            Some("nodes") => (NODES, None),
            Some("matched_edges") if !is_map => (EDGES, Some((&mut self.budget.matched, 100_000))),
            Some("connected_with") if !is_map => {
                (EDGES, Some((&mut self.budget.adjacency, EDGES * 2)))
            }
            Some("documents" | "learning_document_ids" | "doc_hash") => (DOCUMENTS, None),
            Some("terms") => (100, None),
            Some(
                "data"
                | "descriptions"
                | "trigger_descriptions"
                | "ac_reverse_nterm"
                | "aho_corasick_values"
                | "pinnedNodeIds"
                | "pinned_node_ids",
            ) => (NODES, None),
            Some("edges") => (EDGES, None),
            _ => (1_000_000, None),
        };
        if index >= cap || self.budget.entries > 1_000_000 {
            return Err(E::custom(MARKER));
        }
        if let Some((count, limit)) = cumulative {
            *count += 1;
            if *count > limit {
                return Err(E::custom(MARKER));
            }
        }
        Ok(())
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: Error>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: Error>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: Error>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: Error>(self, _: f64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_str<E: Error>(self, _: &str) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> std::result::Result<(), A::Error> {
        let mut i = 0;
        // Ask for a seed only after checking the next member. Empty sequences
        // must not be rejected when a cumulative budget is exactly exhausted.
        while seq
            .next_element_seed(MemberSeed {
                parent: &mut self,
                index: i,
            })?
            .is_some()
        {
            i += 1;
        }
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut i = 0;
        while let Some(key) = map.next_key::<String>()? {
            self.item::<A::Error>(i, true)?;
            i += 1;
            map.next_value_seed(Seed {
                budget: self.budget,
                field: Some(key),
            })?;
        }
        Ok(())
    }
}
struct MemberSeed<'a, 'b> {
    parent: &'a mut Seed<'b>,
    index: usize,
}
impl<'de> DeserializeSeed<'de> for MemberSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        self.parent.item::<D::Error>(self.index, false)?;
        Seed {
            budget: self.parent.budget,
            field: None,
        }
        .deserialize(d)
    }
}
pub(crate) fn validate(text: &str) -> Result<()> {
    let mut d = serde_json::Deserializer::from_str(text);
    let mut budget = Budget::default();
    Seed {
        budget: &mut budget,
        field: None,
    }
    .deserialize(&mut d)
    .and_then(|_| d.end())
    .map_err(|e| {
        if e.to_string().contains(MARKER) {
            error("resource_limit", "JSON collection exceeds supported limits")
        } else {
            invalid()
        }
    })
}
