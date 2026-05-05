use std::time::Instant;

use serde_json::Value;

use crate::{
    core::ids::NodeId,
    surface::card::{Budget, CardCompiler},
};

use super::{
    card_accounting::finalize_card_json,
    helpers::{attach_decision_cards, lift_commentary_text},
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
        let fits = budget_tokens.is_none_or(|cap| token_estimate(&json) <= cap);
        last = Some((json, candidate));
        if fits || candidate == Budget::Tiny {
            break;
        }
    }
    let (json, _) = last.expect("downgrade_budgets returns at least one budget");
    Ok(finalize_card_json(state, json, budget_tokens, start, false))
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
            Ok(serde_json::to_value(&concept)?)
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

fn token_estimate(json: &Value) -> usize {
    json.pointer("/context_accounting/token_estimate")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize
}
