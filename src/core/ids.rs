//! Stable identifier types for graph nodes and edges.
//!
//! Identity stability is the single most important correctness property in
//! synrepo. File node identity survives renames via AST-based detection
//! (see [`crate::structure::identity`]); symbol node identity is keyed on
//! `(file_node_id, qualified_name, kind, body_hash)`.

use std::{error::Error, fmt, str::FromStr};

/// Parse failure for a graph identifier rendered in display form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseIdError {
    kind: &'static str,
    value: String,
}

impl ParseIdError {
    fn new(kind: &'static str, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {} identifier: {}", self.kind, self.value)
    }
}

impl Error for ParseIdError {}

/// Stable identifier for a file node in the graph.
///
/// Derived from the content hash of the first version synrepo ever saw for a
/// given content. Survives renames through AST-based detection. On a rename,
/// the node ID is preserved and a new path entry is appended to the file's
/// path history.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileNodeId(pub u128);

impl fmt::Display for FileNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file_{:032x}", self.0)
    }
}

impl FromStr for FileNodeId {
    type Err = ParseIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_prefixed_u128(value, "file_", "file").map(Self)
    }
}

/// Stable identifier for a symbol node in the graph.
///
/// Derived from `(file_node_id, qualified_name, kind, body_hash)`. The body
/// hash means a symbol whose body is rewritten gets a new identity revision
/// but keeps its logical identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolNodeId(pub u128);

impl fmt::Display for SymbolNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sym_{:032x}", self.0)
    }
}

impl FromStr for SymbolNodeId {
    type Err = ParseIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_prefixed_u128(value, "sym_", "symbol").map(Self)
    }
}

/// Stable identifier for a concept node in the graph.
///
/// Concept nodes are only created from human-authored Markdown files in
/// configured concept directories. In auto mode, if no concept directories
/// exist, there are no ConceptNodeIds in the graph at all — and that's fine,
/// because cards cover the common case without needing an ontology layer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConceptNodeId(pub u128);

impl fmt::Display for ConceptNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "concept_{:032x}", self.0)
    }
}

impl FromStr for ConceptNodeId {
    type Err = ParseIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_prefixed_u128(value, "concept_", "concept").map(Self)
    }
}

/// Unified node ID. Used in graph edges and MCP responses where the node
/// type is determined at runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum NodeId {
    /// A file node.
    File(FileNodeId),
    /// A symbol node.
    Symbol(SymbolNodeId),
    /// A concept node.
    Concept(ConceptNodeId),
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeId::File(id) => write!(f, "{}", id),
            NodeId::Symbol(id) => write!(f, "{}", id),
            NodeId::Concept(id) => write!(f, "{}", id),
        }
    }
}

impl FromStr for NodeId {
    type Err = ParseIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Ok(id) = FileNodeId::from_str(value) {
            return Ok(Self::File(id));
        }
        if let Ok(id) = SymbolNodeId::from_str(value) {
            return Ok(Self::Symbol(id));
        }
        if let Ok(id) = ConceptNodeId::from_str(value) {
            return Ok(Self::Concept(id));
        }

        Err(ParseIdError::new("node", value))
    }
}

/// Stable identifier for a graph edge.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EdgeId(pub u128);

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "edge_{:032x}", self.0)
    }
}

impl FromStr for EdgeId {
    type Err = ParseIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_prefixed_u128(value, "edge_", "edge").map(Self)
    }
}

fn parse_prefixed_u128(
    value: &str,
    prefix: &'static str,
    kind: &'static str,
) -> Result<u128, ParseIdError> {
    let hex = value
        .strip_prefix(prefix)
        .ok_or_else(|| ParseIdError::new(kind, value))?;

    u128::from_str_radix(hex, 16).map_err(|_| ParseIdError::new(kind, value))
}

#[cfg(test)]
mod tests {
    use super::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};

    #[test]
    fn ids_round_trip_through_display_format() {
        let file = FileNodeId(0x42);
        let symbol = SymbolNodeId(0x24);
        let concept = ConceptNodeId(0x99);
        let edge = EdgeId(0x77);

        assert_eq!(
            "file_00000000000000000000000000000042"
                .parse::<FileNodeId>()
                .unwrap(),
            file
        );
        assert_eq!(
            "sym_00000000000000000000000000000024"
                .parse::<SymbolNodeId>()
                .unwrap(),
            symbol
        );
        assert_eq!(
            "concept_00000000000000000000000000000099"
                .parse::<ConceptNodeId>()
                .unwrap(),
            concept
        );
        assert_eq!(
            "edge_00000000000000000000000000000077"
                .parse::<EdgeId>()
                .unwrap(),
            edge
        );
        assert_eq!(
            file.to_string().parse::<NodeId>().unwrap(),
            NodeId::File(file)
        );
        assert_eq!(
            symbol.to_string().parse::<NodeId>().unwrap(),
            NodeId::Symbol(symbol)
        );
        assert_eq!(
            concept.to_string().parse::<NodeId>().unwrap(),
            NodeId::Concept(concept)
        );
    }

    #[test]
    fn invalid_ids_fail_cleanly() {
        assert!("file_nothex".parse::<FileNodeId>().is_err());
        assert!("unknown_0001".parse::<NodeId>().is_err());
    }
}

use serde::{Deserialize, Deserializer, Serialize, Serializer};

impl Serialize for FileNodeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FileNodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for SymbolNodeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SymbolNodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for ConceptNodeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConceptNodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for EdgeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EdgeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
