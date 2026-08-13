use std::borrow::Cow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sentry::protocol::{DebugMeta, Event, Level, Map};

const SENTRY_DSN_ENV: &str = "SYNREPO_SENTRY_DSN";
const DEFAULT_SENTRY_DSN: &str =
    "https://cfb2726a0524a23eefdd59c7b89e4aef@o4511520494190592.ingest.us.sentry.io/4511520498712576";
const EVENT_MESSAGE: &str = "synrepo MCP tool call failed";
const LOGGER: &str = "synrepo.mcp.telemetry";
const MAX_TAG_VALUE_BYTES: usize = 64;

static ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn init_from_config_and_env(repo_root: &Path) -> Option<sentry::ClientInitGuard> {
    match synrepo::config::Config::load(repo_root) {
        Ok(config) if config.mcp_sentry_telemetry_enabled() => {}
        Ok(_) => {
            ENABLED.store(false, Ordering::Relaxed);
            return None;
        }
        Err(error) => {
            ENABLED.store(false, Ordering::Relaxed);
            tracing::debug!(
                error = %error,
                "Sentry MCP telemetry disabled because synrepo config could not be loaded"
            );
            return None;
        }
    }

    let Some(dsn_text) = dsn_text_from_env_or_default() else {
        ENABLED.store(false, Ordering::Relaxed);
        return None;
    };
    if dsn_text.as_ref().parse::<sentry::types::Dsn>().is_err() {
        ENABLED.store(false, Ordering::Relaxed);
        if std::env::var_os(SENTRY_DSN_ENV).is_some() {
            tracing::warn!("{SENTRY_DSN_ENV} is set but invalid; Sentry MCP telemetry disabled");
        } else {
            tracing::warn!("default Sentry DSN is invalid; Sentry MCP telemetry disabled");
        }
        return None;
    }

    let options = sentry::ClientOptions::new()
        .dsn(dsn_text.as_ref())
        .send_default_pii(false)
        .attach_stacktrace(false)
        .traces_sample_rate(0.0)
        .max_breadcrumbs(0)
        .default_integrations(false)
        // auto_session_tracking's setter needs the "release-health" feature,
        // which is not enabled; the field already defaults to false.
        .enable_logs(false)
        .enable_metrics(false)
        .shutdown_timeout(Duration::from_millis(750))
        .before_send(|event| Some(scrub_event(event)))
        .before_breadcrumb(|_| None);
    let guard = sentry::init(options);
    ENABLED.store(true, Ordering::Relaxed);
    Some(guard)
}

fn dsn_text_from_env_or_default() -> Option<Cow<'static, str>> {
    let raw = match std::env::var(SENTRY_DSN_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Some(Cow::Borrowed(DEFAULT_SENTRY_DSN)),
        Err(std::env::VarError::NotUnicode(_)) => return None,
    };
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    Some(Cow::Owned(trimmed))
}

pub(crate) fn capture_failed_tool_call(tool: &str, error_code: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    sentry::capture_event(failed_tool_event(tool, error_code));
}

fn failed_tool_event(tool: &str, error_code: &str) -> Event<'static> {
    let tags = tags_for(tool, error_code);
    let tool = tags
        .get("tool")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let error_code = tags
        .get("error_code")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    Event {
        level: Level::Error,
        message: Some(EVENT_MESSAGE.to_string()),
        logger: Some(LOGGER.to_string()),
        fingerprint: fingerprint_for(tool, error_code),
        tags,
        ..Default::default()
    }
}

fn scrub_event(mut event: Event<'static>) -> Event<'static> {
    event.tags = allowlisted_tags(event.tags);
    let tool = event
        .tags
        .get("tool")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let error_code = event
        .tags
        .get("error_code")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    event.message = Some(EVENT_MESSAGE.to_string());
    event.logger = Some(LOGGER.to_string());
    event.fingerprint = fingerprint_for(tool, error_code);
    event.logentry = None;
    event.platform = Cow::Borrowed("other");
    event.transaction = None;
    event.culprit = None;
    event.server_name = None;
    event.release = None;
    event.dist = None;
    event.environment = None;
    event.user = None;
    event.request = None;
    event.contexts.clear();
    event.breadcrumbs.values.clear();
    event.exception.values.clear();
    event.stacktrace = None;
    event.template = None;
    event.threads.values.clear();
    event.modules.clear();
    event.extra.clear();
    event.debug_meta = Cow::Owned(DebugMeta::default());
    event.sdk = None;
    event
}

fn tags_for(tool: &str, error_code: &str) -> Map<String, String> {
    let mut tags = Map::new();
    tags.insert("component".to_string(), "mcp".to_string());
    tags.insert("tool".to_string(), safe_tag_value(tool));
    tags.insert("error_code".to_string(), safe_tag_value(error_code));
    tags.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    tags.insert("os".to_string(), std::env::consts::OS.to_string());
    tags.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    tags
}

fn allowlisted_tags(input: Map<String, String>) -> Map<String, String> {
    tags_for(
        input.get("tool").map(String::as_str).unwrap_or("unknown"),
        input
            .get("error_code")
            .map(String::as_str)
            .unwrap_or("unknown"),
    )
}

fn fingerprint_for(tool: String, error_code: String) -> Cow<'static, [Cow<'static, str>]> {
    Cow::Owned(vec![
        Cow::Borrowed("mcp_tool_failed"),
        Cow::Owned(tool),
        Cow::Owned(error_code),
    ])
}

fn safe_tag_value(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if out.len() >= MAX_TAG_VALUE_BYTES {
            break;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use sentry::protocol::{Breadcrumb, Request, User};

    use super::*;

    #[test]
    fn failed_tool_event_contains_only_bounded_tags() {
        let event = failed_tool_event("synrepo_card", "NOT_FOUND");

        assert_eq!(event.message.as_deref(), Some(EVENT_MESSAGE));
        assert_eq!(event.logger.as_deref(), Some(LOGGER));
        assert_eq!(event.level, Level::Error);
        assert_eq!(
            event.tags.get("tool").map(String::as_str),
            Some("synrepo_card")
        );
        assert_eq!(
            event.tags.get("error_code").map(String::as_str),
            Some("NOT_FOUND")
        );
        assert!(event.user.is_none());
        assert!(event.request.is_none());
        assert!(event.contexts.is_empty());
        assert!(event.extra.is_empty());
        assert!(event.breadcrumbs.is_empty());
    }

    #[test]
    fn scrub_event_drops_sensitive_fields_and_tag_values() {
        let mut event = failed_tool_event("synrepo_search", "INVALID_PARAMETER");
        event.fingerprint = Cow::Owned(vec![Cow::Borrowed("private fingerprint")]);
        event.logentry = Some(sentry::protocol::LogEntry {
            message: "private logentry".to_string(),
            params: Vec::new(),
        });
        event.server_name = Some(Cow::Borrowed("local-hostname"));
        event.release = Some(Cow::Borrowed("private-release"));
        event.environment = Some(Cow::Borrowed("private-env"));
        event.user = Some(User {
            username: Some("local-user".to_string()),
            ..Default::default()
        });
        event.request = Some(Request {
            url: "https://example.invalid/private?q=secret".parse().ok(),
            ..Default::default()
        });
        event.breadcrumbs.values.push(Breadcrumb {
            message: Some("target private_symbol".to_string()),
            ..Default::default()
        });
        event
            .extra
            .insert("query".to_string(), "private query".into());
        event
            .tags
            .insert("repo_root".to_string(), "/tmp/repo".to_string());
        event
            .tags
            .insert("version".to_string(), "private-version".to_string());
        event.tags.insert(
            "tool".to_string(),
            "tool with spaces and a very very very very very very very long suffix".to_string(),
        );

        let scrubbed = scrub_event(event);

        assert!(scrubbed.user.is_none());
        assert!(scrubbed.request.is_none());
        assert!(scrubbed.logentry.is_none());
        assert!(scrubbed.server_name.is_none());
        assert!(scrubbed.release.is_none());
        assert!(scrubbed.environment.is_none());
        assert!(scrubbed.breadcrumbs.is_empty());
        assert!(scrubbed.extra.is_empty());
        assert!(!scrubbed.tags.contains_key("repo_root"));
        assert_eq!(
            scrubbed.tags.get("component").map(String::as_str),
            Some("mcp")
        );
        assert_eq!(
            scrubbed.tags.get("version").map(String::as_str),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            scrubbed.tags.get("tool").map(String::as_str),
            Some("tool_with_spaces_and_a_very_very_very_very_very_very_very_long_s")
        );
        assert_eq!(
            scrubbed.fingerprint.as_ref(),
            [
                Cow::Borrowed("mcp_tool_failed"),
                Cow::Borrowed("tool_with_spaces_and_a_very_very_very_very_very_very_very_long_s"),
                Cow::Borrowed("INVALID_PARAMETER"),
            ]
        );
    }

    #[test]
    fn dsn_text_uses_default_when_env_is_absent() {
        let _lock = synrepo::test_support::global_test_lock("sentry-dsn-env");
        let _guard = EnvVarGuard::unset(SENTRY_DSN_ENV);

        assert_eq!(
            dsn_text_from_env_or_default().as_deref(),
            Some(DEFAULT_SENTRY_DSN)
        );
    }

    #[test]
    fn dsn_text_prefers_env_when_present() {
        let _lock = synrepo::test_support::global_test_lock("sentry-dsn-env");
        let _guard = EnvVarGuard::set(
            SENTRY_DSN_ENV,
            " https://public@example.invalid/123456 ".to_string(),
        );

        assert_eq!(
            dsn_text_from_env_or_default().as_deref(),
            Some("https://public@example.invalid/123456")
        );
    }

    #[test]
    fn dsn_text_empty_env_disables_fallback() {
        let _lock = synrepo::test_support::global_test_lock("sentry-dsn-env");
        let _guard = EnvVarGuard::set(SENTRY_DSN_ENV, "   ".to_string());

        assert!(dsn_text_from_env_or_default().is_none());
    }

    struct EnvVarGuard {
        name: &'static str,
        original: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: String) -> Self {
            let guard = Self {
                name,
                original: std::env::var_os(name),
            };
            std::env::set_var(name, value);
            guard
        }

        fn unset(name: &'static str) -> Self {
            let guard = Self {
                name,
                original: std::env::var_os(name),
            };
            std::env::remove_var(name);
            guard
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.original.as_ref() {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }
}
