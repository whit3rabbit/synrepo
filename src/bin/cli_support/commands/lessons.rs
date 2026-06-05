use std::{path::Path, str::FromStr};

use synrepo::{
    config::Config,
    overlay::{AgentNoteConfidence, AgentNoteTarget, AgentNoteTargetKind},
    pipeline::writer::{acquire_write_admission, map_lock_error},
    store::overlay::SqliteOverlayStore,
    surface::lessons::{self, LessonAdd, LessonQuery, LessonView, DEFAULT_LESSON_LIMIT},
};

pub(crate) fn remember(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonRememberArgs,
) -> anyhow::Result<()> {
    print!("{}", remember_output(repo_root, args)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn remember_output(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonRememberArgs,
) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "remember")
        .map_err(|err| map_lock_error("remember", err))?;
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay"))?;
    let target = resolve_lesson_target(args.target_kind.as_deref(), args.target.as_deref())?;
    let ttl_seconds = args
        .ttl
        .as_deref()
        .map(lessons::parse_cli_ttl)
        .transpose()?;
    let lesson = lessons::add_lesson(
        Some(repo_root),
        &mut overlay,
        LessonAdd {
            target_kind: target.kind,
            target: target.id,
            claim: args.claim,
            created_by: args.actor,
            confidence: AgentNoteConfidence::from_str(&args.confidence)?,
            evidence: lessons::text_evidence(&args.evidence)?,
            source_hashes: Vec::new(),
            graph_revision: None,
            ttl_seconds,
        },
    )?;
    render_lesson_mutation("remembered", &lesson, args.json)
}

pub(crate) fn recall(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonRecallArgs,
) -> anyhow::Result<()> {
    print!("{}", recall_output(repo_root, args)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn recall_output(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonRecallArgs,
) -> anyhow::Result<String> {
    let lessons = query_lessons(
        repo_root,
        Some(args.query),
        args.target_kind.as_deref(),
        args.target.as_deref(),
        args.limit,
        args.include_hidden,
    )?;
    render_lessons(&lessons, args.json)
}

pub(crate) fn list(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonListArgs,
) -> anyhow::Result<()> {
    print!("{}", list_output(repo_root, args)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn list_output(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonListArgs,
) -> anyhow::Result<String> {
    let lessons = query_lessons(
        repo_root,
        None,
        args.target_kind.as_deref(),
        args.target.as_deref(),
        args.limit,
        args.include_hidden,
    )?;
    render_lessons(&lessons, args.json)
}

pub(crate) fn forget(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonForgetArgs,
) -> anyhow::Result<()> {
    print!("{}", forget_output(repo_root, args)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn forget_output(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonForgetArgs,
) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "forget lesson")
        .map_err(|err| map_lock_error("forget lesson", err))?;
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let lesson = lessons::forget_lesson(
        Some(repo_root),
        &mut overlay,
        &args.lesson_id,
        &args.actor,
        args.reason.as_deref(),
    )?;
    render_lesson_mutation("forgotten", &lesson, args.json)
}

pub(crate) fn verify(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonVerifyArgs,
) -> anyhow::Result<()> {
    print!("{}", verify_output(repo_root, args)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn verify_output(
    repo_root: &Path,
    args: crate::cli_support::cli_args::LessonVerifyArgs,
) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "verify lesson")
        .map_err(|err| map_lock_error("verify lesson", err))?;
    let mut overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let lesson = lessons::verify_lesson(
        Some(repo_root),
        &mut overlay,
        &args.lesson_id,
        &args.actor,
        None,
    )?;
    render_lesson_mutation("verified", &lesson, args.json)
}

fn query_lessons(
    repo_root: &Path,
    search: Option<String>,
    target_kind: Option<&str>,
    target: Option<&str>,
    limit: Option<usize>,
    include_hidden: bool,
) -> anyhow::Result<Vec<LessonView>> {
    let overlay = lessons::open_existing_lessons_overlay(repo_root)?;
    let (target_kind, target) = resolve_query_target(target_kind, target)?;
    Ok(lessons::search_lessons(
        Some(repo_root),
        &overlay,
        LessonQuery {
            query: search,
            target_kind,
            target,
            limit: limit.unwrap_or(DEFAULT_LESSON_LIMIT),
            include_hidden,
        },
    )?)
}

fn resolve_lesson_target(
    kind: Option<&str>,
    target: Option<&str>,
) -> anyhow::Result<AgentNoteTarget> {
    let kind = match kind {
        Some(value) => AgentNoteTargetKind::from_str(value)?,
        None if target.is_none() => AgentNoteTargetKind::Repo,
        None => AgentNoteTargetKind::Path,
    };
    let id = match (kind, target) {
        (AgentNoteTargetKind::Repo, None) => ".".to_string(),
        (_, Some(target)) if !target.trim().is_empty() => target.trim().to_string(),
        _ => anyhow::bail!("--target is required unless --target-kind repo is used"),
    };
    Ok(AgentNoteTarget { kind, id })
}

fn resolve_query_target(
    kind: Option<&str>,
    target: Option<&str>,
) -> anyhow::Result<(Option<AgentNoteTargetKind>, Option<String>)> {
    let target_kind = kind.map(AgentNoteTargetKind::from_str).transpose()?;
    let target_id = target.map(|value| value.trim().to_string());
    let target_kind = match (target_kind, target_id.as_deref()) {
        (None, Some(_)) => Some(AgentNoteTargetKind::Path),
        (other, _) => other,
    };
    Ok((target_kind, target_id))
}

fn render_lesson_mutation(
    action: &str,
    lesson: &LessonView,
    json_output: bool,
) -> anyhow::Result<String> {
    if json_output {
        return Ok(format!("{}\n", serde_json::to_string_pretty(lesson)?));
    }
    Ok(format!(
        "Lesson {}: {} [{}]\n",
        action, lesson.lesson_id, lesson.freshness
    ))
}

fn render_lessons(lessons: &[LessonView], json_output: bool) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    if json_output {
        return Ok(format!("{}\n", serde_json::to_string_pretty(lessons)?));
    }
    let mut out = String::new();
    writeln!(out, "Found {} lessons.", lessons.len()).unwrap();
    for lesson in lessons {
        writeln!(
            out,
            "{} [{}/{}] {}:{}",
            lesson.lesson_id, lesson.status, lesson.freshness, lesson.target_kind, lesson.target
        )
        .unwrap();
        writeln!(out, "  {}", lesson.claim).unwrap();
    }
    Ok(out)
}
