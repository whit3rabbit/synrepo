use std::time::Instant;

use serde_json::Value;

use crate::{
    core::ids::NodeId,
    surface::card::{concept::concept_summary_card, Budget, CardCompiler},
};

use super::{
    card_accounting::{prepare_card_json, record_embedded_card_metrics},
    helpers::{attach_decision_cards, lift_commentary_text},
    response_budget::estimate_json_tokens,
    SynrepoState,
};

pub fn render_card_target(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
    budget_tokens: Option<usize>,
    include_notes: bool,
    start: Instant,
) -> anyhow::Result<Value> {
    let mut last = None;
    for &candidate in downgrade_budgets(budget) {
        let json = render_card_content(state, compiler, target, candidate, include_notes)?;
        let json = prepare_card_json(state, json, budget_tokens);
        let fits = fits_budget_tokens(&json, budget_tokens);
        last = Some((json, candidate));
        if fits || candidate == Budget::Tiny {
            break;
        }
    }
    let (json, _) = last.expect("downgrade_budgets returns at least one budget");
    record_embedded_card_metrics(state, &json, start, false);
    Ok(json)
}

fn render_card_content(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
    budget: Budget,
    include_notes: bool,
) -> anyhow::Result<Value> {
    let node_id = compiler
        .resolve_target(target)?
        .ok_or_else(|| super::error::McpError::not_found(format!("target not found: {target}")))?;

    match node_id {
        NodeId::Symbol(sym_id) => {
            let card = compiler.symbol_card(sym_id, budget)?;
            let mut json_val = serde_json::to_value(&card)?;
            lift_commentary_text(&mut json_val);
            attach_decision_cards(
                &mut json_val,
                NodeId::Symbol(sym_id),
                compiler.reader(),
                budget,
            )?;
            if include_notes {
                super::notes::attach_agent_notes(state, &mut json_val, NodeId::Symbol(sym_id))?;
            }
            Ok(json_val)
        }
        NodeId::File(file_id) => {
            let card = compiler.file_card(file_id, budget)?;
            let mut json_val = serde_json::to_value(&card)?;
            attach_decision_cards(
                &mut json_val,
                NodeId::File(file_id),
                compiler.reader(),
                budget,
            )?;
            if include_notes {
                super::notes::attach_agent_notes(state, &mut json_val, NodeId::File(file_id))?;
            }
            Ok(json_val)
        }
        NodeId::Concept(concept_id) => {
            let concept = compiler
                .reader()
                .get_concept(concept_id)?
                .ok_or_else(|| anyhow::anyhow!("concept not found"))?;
            Ok(concept_summary_card(&concept))
        }
    }
}

fn downgrade_budgets(budget: Budget) -> &'static [Budget] {
    match budget {
        Budget::Deep => &[Budget::Deep, Budget::Normal, Budget::Tiny],
        Budget::Normal => &[Budget::Normal, Budget::Tiny],
        Budget::Tiny => &[Budget::Tiny],
    }
}

fn fits_budget_tokens(json: &Value, budget_tokens: Option<usize>) -> bool {
    budget_tokens.is_none_or(|cap| estimate_json_tokens(json) <= cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_fit_uses_final_serialized_json_size() {
        let json = serde_json::json!({
            "context_accounting": {
                "token_estimate": 1
            },
            "payload": "x".repeat(120)
        });
        let serialized_estimate = estimate_json_tokens(&json);

        assert!(serialized_estimate > 1);
        assert!(!fits_budget_tokens(&json, Some(1)));
        assert!(fits_budget_tokens(&json, Some(serialized_estimate)));
    }
}
