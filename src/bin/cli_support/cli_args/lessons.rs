use clap::Args;

#[derive(Args)]
pub(crate) struct LessonRememberArgs {
    /// Saved lesson claim.
    pub(crate) claim: String,
    /// Optional target ID or repo-relative path.
    #[arg(long)]
    pub(crate) target: Option<String>,
    /// Target kind: repo, path, file, symbol, concept, test, card, note.
    #[arg(long)]
    pub(crate) target_kind: Option<String>,
    /// Expire the lesson after a duration such as 30d, 12h, 2w, or 15m.
    #[arg(long)]
    pub(crate) ttl: Option<String>,
    /// Bounded evidence text. May be repeated.
    #[arg(long)]
    pub(crate) evidence: Vec<String>,
    /// Author/tool identity.
    #[arg(long, default_value = "cli-user")]
    pub(crate) actor: String,
    /// Confidence: low, medium, high.
    #[arg(long, default_value = "medium")]
    pub(crate) confidence: String,
    /// Emit JSON instead of human-readable output.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct LessonRecallArgs {
    /// Query to match against saved lesson claims, targets, and evidence.
    pub(crate) query: String,
    /// Optional target ID or repo-relative path filter.
    #[arg(long)]
    pub(crate) target: Option<String>,
    /// Optional target kind filter.
    #[arg(long)]
    pub(crate) target_kind: Option<String>,
    /// Maximum lessons to return.
    #[arg(long)]
    pub(crate) limit: Option<usize>,
    /// Include forgotten, superseded, invalid, and expired lessons.
    #[arg(long)]
    pub(crate) include_hidden: bool,
    /// Emit JSON instead of human-readable output.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct LessonListArgs {
    /// Optional target ID or repo-relative path filter.
    #[arg(long)]
    pub(crate) target: Option<String>,
    /// Optional target kind filter.
    #[arg(long)]
    pub(crate) target_kind: Option<String>,
    /// Maximum lessons to return.
    #[arg(long)]
    pub(crate) limit: Option<usize>,
    /// Include forgotten, superseded, invalid, and expired lessons.
    #[arg(long)]
    pub(crate) include_hidden: bool,
    /// Emit JSON instead of human-readable output.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct LessonForgetArgs {
    /// Lesson ID.
    pub(crate) lesson_id: String,
    /// Actor identity.
    #[arg(long, default_value = "cli-user")]
    pub(crate) actor: String,
    /// Optional reason.
    #[arg(long)]
    pub(crate) reason: Option<String>,
    /// Emit JSON instead of human-readable output.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct LessonVerifyArgs {
    /// Lesson ID.
    pub(crate) lesson_id: String,
    /// Actor identity.
    #[arg(long, default_value = "cli-user")]
    pub(crate) actor: String,
    /// Emit JSON instead of human-readable output.
    #[arg(long)]
    pub(crate) json: bool,
}
