use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::pipeline::explain::telemetry::{publish_budget_blocked, CallCtx, ExplainTarget};
use crate::pipeline::explain::{
    CommentaryEntry, CommentaryFuture, CommentaryGeneration, CommentaryGenerator,
    CommentarySkipReason,
};

use super::wire::{ChatMessage, ChatRequest, ChatResponse};
use super::{OpenAiCompatProvider, COMMENTARY_MAX_OUTPUT_TOKENS, COMMENTARY_SYSTEM_PROMPT};
use crate::pipeline::explain::providers::http::{
    cap_output_bytes, estimate_tokens, post_json_strict, post_json_strict_async, resolve_usage,
    UsageResolution,
};
use crate::pipeline::explain::providers::shared::sanitize_generated_commentary_text;

impl OpenAiCompatProvider {
    fn commentary_body<'a>(&'a self, context: &'a str) -> ChatRequest<'a> {
        ChatRequest {
            model: &self.model,
            max_tokens: COMMENTARY_MAX_OUTPUT_TOKENS,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: COMMENTARY_SYSTEM_PROMPT,
                },
                ChatMessage {
                    role: "user",
                    content: context,
                },
            ],
        }
    }

    fn finish_commentary(
        &self,
        node: NodeId,
        parsed: ChatResponse,
        headers: &[(&str, &str)],
        estimated_tokens: u32,
        ctx: CallCtx,
    ) -> Option<CommentaryEntry> {
        let extras = self.resolve_extras(&parsed, headers);
        self.finish_commentary_with_extras(node, parsed, extras, estimated_tokens, ctx)
    }

    async fn finish_commentary_async(
        &self,
        node: NodeId,
        parsed: ChatResponse,
        headers: &[(&str, &str)],
        estimated_tokens: u32,
        ctx: CallCtx,
    ) -> Option<CommentaryEntry> {
        let extras = self.resolve_extras_async(&parsed, headers).await;
        self.finish_commentary_with_extras(node, parsed, extras, estimated_tokens, ctx)
    }

    fn finish_commentary_with_extras(
        &self,
        node: NodeId,
        parsed: ChatResponse,
        extras: super::ResponseExtras,
        estimated_tokens: u32,
        ctx: CallCtx,
    ) -> Option<CommentaryEntry> {
        let reported_raw = parsed
            .usage
            .as_ref()
            .map(|u| (u.prompt_tokens, u.completion_tokens));
        let raw_text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();
        let Some(text) = sanitize_generated_commentary_text(&raw_text) else {
            let reported = extras.usage_override.or(reported_raw);
            let usage = resolve_usage(UsageResolution::from_output_text(
                reported,
                estimated_tokens,
                "",
            ));
            ctx.complete_with_cost(usage, extras.billed_cost, 0);
            return None;
        };

        let reported = extras.usage_override.or(reported_raw);
        let usage = resolve_usage(UsageResolution::from_output_text(
            reported,
            estimated_tokens,
            &text,
        ));
        ctx.complete_with_cost(usage, extras.billed_cost, cap_output_bytes(&text));

        if text.is_empty() {
            return None;
        }

        Some(CommentaryEntry {
            node_id: node,
            text,
            provenance: crate::overlay::CommentaryProvenance {
                source_content_hash: String::new(),
                pass_id: self.config.pass_id.to_string(),
                model_identity: self.model.clone(),
                generated_at: OffsetDateTime::now_utc(),
            },
        })
    }

    fn generate_commentary(
        &self,
        node: NodeId,
        context: &str,
    ) -> crate::Result<Option<CommentaryEntry>> {
        let target = ExplainTarget::Commentary { node };
        let estimated_tokens = estimate_tokens(context);
        if estimated_tokens > self.max_tokens_per_call {
            publish_budget_blocked(
                self.config.provider,
                &self.model,
                target,
                estimated_tokens,
                self.max_tokens_per_call,
            );
            return Ok(None);
        }

        let body = self.commentary_body(context);
        let auth_header = format!("Bearer {}", self.api_key);
        let headers = self.build_headers(&auth_header);
        let ctx = CallCtx::start(self.config.provider, &self.model, target);
        let parsed = match post_json_strict(&self.client, self.config.api_url, &headers, &body) {
            Ok(parsed) => parsed,
            Err(error) => {
                ctx.fail(error);
                return Ok(None);
            }
        };
        Ok(self.finish_commentary(node, parsed, &headers, estimated_tokens, ctx))
    }

    async fn generate_commentary_async(
        &self,
        node: NodeId,
        context: &str,
    ) -> crate::Result<Option<CommentaryEntry>> {
        let target = ExplainTarget::Commentary { node };
        let estimated_tokens = estimate_tokens(context);
        if estimated_tokens > self.max_tokens_per_call {
            publish_budget_blocked(
                self.config.provider,
                &self.model,
                target,
                estimated_tokens,
                self.max_tokens_per_call,
            );
            return Ok(None);
        }

        let body = self.commentary_body(context);
        let auth_header = format!("Bearer {}", self.api_key);
        let headers = self.build_headers(&auth_header);
        let ctx = CallCtx::start(self.config.provider, &self.model, target);
        let parsed =
            match post_json_strict_async(&self.async_client, self.config.api_url, &headers, &body)
                .await
            {
                Ok(parsed) => parsed,
                Err(error) => {
                    ctx.fail(error);
                    return Ok(None);
                }
            };
        Ok(self
            .finish_commentary_async(node, parsed, &headers, estimated_tokens, ctx)
            .await)
    }
}

impl CommentaryGenerator for OpenAiCompatProvider {
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>> {
        self.generate_commentary(node, context)
    }

    fn generate_with_outcome_async<'a>(
        &'a self,
        node: NodeId,
        context: &'a str,
    ) -> CommentaryFuture<'a> {
        Box::pin(async move {
            self.generate_commentary_async(node, context)
                .await
                .map(|entry| {
                    CommentaryGeneration::from_optional(entry, CommentarySkipReason::Unknown)
                })
        })
    }
}
