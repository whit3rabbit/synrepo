use super::action_handlers::watch_toggle_label_for;
use super::AppMode;
use crate::surface::status_snapshot::StatusSnapshot;
use crate::tui::widgets::QuickAction;

pub(super) fn quick_actions_for(mode: &AppMode, snapshot: &StatusSnapshot) -> Vec<QuickAction> {
    let mut actions = vec![QuickAction {
        key: "r".to_string(),
        label: "refresh snapshot".to_string(),
        disabled: false,
        requires_confirm: false,
        destructive: false,
        expensive: false,
        command_label: Some("refresh snapshot".to_string()),
    }];
    if can_generate_graph(snapshot) {
        actions.push(QuickAction {
            key: "M".to_string(),
            label: "generate graph".to_string(),
            disabled: false,
            requires_confirm: true,
            destructive: false,
            expensive: true,
            command_label: Some("materialize graph".to_string()),
        });
    }
    if let Some(watch_label) = watch_toggle_label_for(mode, snapshot) {
        actions.push(QuickAction {
            key: "w".to_string(),
            label: format!("{watch_label} watch"),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some(format!("watch {watch_label} current")),
        });
    }
    if snapshot_has_pending_compatibility_action(snapshot) {
        actions.push(QuickAction {
            key: "U".to_string(),
            label: "apply compatibility".to_string(),
            disabled: false,
            requires_confirm: true,
            destructive: false,
            expensive: true,
            command_label: Some("apply compatibility".to_string()),
        });
    }
    if snapshot.initialized {
        let embeddings_enabled = snapshot
            .config
            .as_ref()
            .map(|config| config.enable_semantic_triage)
            .unwrap_or(false);
        let label = if embeddings_enabled {
            "disable embeddings"
        } else {
            "enable optional embeddings"
        };
        actions.push(QuickAction {
            key: "T".to_string(),
            label: label.to_string(),
            disabled: false,
            requires_confirm: embeddings_enabled,
            destructive: false,
            expensive: !embeddings_enabled,
            command_label: Some(label.to_string()),
        });
        if embeddings_enabled {
            actions.push(QuickAction {
                key: "B".to_string(),
                label: "build embeddings".to_string(),
                disabled: false,
                requires_confirm: false,
                destructive: false,
                expensive: true,
                command_label: Some("build embeddings".to_string()),
            });
        }
    }
    actions.extend([
        QuickAction {
            key: "i".to_string(),
            label: "agent integration".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some("agent integration".to_string()),
        },
        QuickAction {
            key: "e".to_string(),
            label: "configure optional explain".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some("configure optional explain".to_string()),
        },
        QuickAction {
            key: "q".to_string(),
            label: "quit".to_string(),
            disabled: false,
            requires_confirm: false,
            destructive: false,
            expensive: false,
            command_label: Some("quit".to_string()),
        },
    ]);
    actions
}

pub(super) fn can_generate_graph(snapshot: &StatusSnapshot) -> bool {
    snapshot.initialized && snapshot.graph_stats.is_none()
}

fn snapshot_has_pending_compatibility_action(snapshot: &StatusSnapshot) -> bool {
    snapshot.diagnostics.as_ref().is_some_and(|diag| {
        diag.store_guidance
            .iter()
            .any(|guidance| is_applicable_compatibility_guidance(guidance))
    })
}

fn is_applicable_compatibility_guidance(guidance: &str) -> bool {
    guidance.contains("needs rebuild")
        || guidance.contains("needs invalidation")
        || guidance.contains("needs clear-and-recreate")
}
