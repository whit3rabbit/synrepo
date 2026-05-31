use std::collections::{BTreeMap, BTreeSet};

use crate::structure::graph::{SymbolNode, Visibility};

mod javascript;
mod python;
mod scan;

#[derive(Default)]
pub(crate) struct MissingDocsApiIndex {
    python_reexports: BTreeMap<String, BTreeSet<String>>,
}

impl MissingDocsApiIndex {
    pub(crate) fn record_python_init(&mut self, path: &str, source: &[u8]) {
        if !python::is_init(path) {
            return;
        }
        let source = String::from_utf8_lossy(source);
        for (target_path, names) in python::relative_reexports(path, &source) {
            self.python_reexports
                .entry(target_path)
                .or_default()
                .extend(names);
        }
    }

    fn python_reexports_for(&self, path: &str) -> BTreeSet<String> {
        self.python_reexports.get(path).cloned().unwrap_or_default()
    }
}

pub(crate) struct DocsRequiredApi {
    kind: ApiKind,
}

enum ApiKind {
    Default,
    Javascript(BTreeSet<String>),
    Python {
        all_names: BTreeSet<String>,
        reexported_names: BTreeSet<String>,
        is_init: bool,
    },
}

impl DocsRequiredApi {
    pub(crate) fn for_file(
        path: &str,
        language: Option<&str>,
        source: &[u8],
        index: &MissingDocsApiIndex,
    ) -> Self {
        let source = String::from_utf8_lossy(source);
        let kind = match language {
            Some("javascript") => ApiKind::Javascript(javascript::exported_names(&source)),
            Some("python") => ApiKind::Python {
                all_names: python::all_names(&source),
                reexported_names: index.python_reexports_for(path),
                is_init: python::is_init(path),
            },
            _ => ApiKind::Default,
        };
        Self { kind }
    }

    pub(crate) fn requires_docs(&self, symbol: &SymbolNode) -> bool {
        match &self.kind {
            ApiKind::Default => symbol.visibility == Visibility::Public,
            ApiKind::Javascript(exports) => exports.contains(&symbol.display_name),
            ApiKind::Python {
                all_names,
                reexported_names,
                is_init,
            } => {
                (*is_init && symbol.visibility == Visibility::Public)
                    || all_names.contains(&symbol.display_name)
                    || reexported_names.contains(&symbol.display_name)
            }
        }
    }
}
