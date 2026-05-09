use super::apply::AnchorEditRequest;

#[derive(Clone, Debug)]
enum PlannedKind {
    InsertBefore,
    InsertAfter,
    Replace,
    Delete,
}

#[derive(Clone, Debug)]
pub(super) struct PlannedEdit {
    pub(super) start: usize,
    pub(super) end: usize,
    kind: PlannedKind,
    text: Vec<String>,
}

impl PlannedEdit {
    pub(super) fn from_request(
        edit: &AnchorEditRequest,
        start: usize,
        end: usize,
    ) -> anyhow::Result<Self> {
        let kind = match edit.edit_type.as_str() {
            "insert" | "insert_after" => PlannedKind::InsertAfter,
            "insert_before" => PlannedKind::InsertBefore,
            "replace" => PlannedKind::Replace,
            "delete" => PlannedKind::Delete,
            other => anyhow::bail!("unsupported edit_type: {other}"),
        };
        let text = match kind {
            PlannedKind::Delete => Vec::new(),
            _ => edit
                .text
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("text is required for {}", edit.edit_type))?
                .lines()
                .map(ToString::to_string)
                .collect(),
        };
        Ok(Self {
            start,
            end,
            kind,
            text,
        })
    }

    fn interval(&self) -> (usize, usize) {
        match self.kind {
            PlannedKind::InsertBefore => (self.start, self.start),
            PlannedKind::InsertAfter => (self.start + 1, self.start + 1),
            PlannedKind::Replace | PlannedKind::Delete => (self.start, self.end + 1),
        }
    }

    pub(super) fn descending_apply_key(&self) -> (usize, usize) {
        (self.start, self.end)
    }

    pub(super) fn apply(self, lines: &mut Vec<String>) {
        match self.kind {
            PlannedKind::InsertBefore => {
                lines.splice(self.start..self.start, self.text);
            }
            PlannedKind::InsertAfter => {
                let idx = self.start + 1;
                lines.splice(idx..idx, self.text);
            }
            PlannedKind::Replace => {
                lines.splice(self.start..=self.end, self.text);
            }
            PlannedKind::Delete => {
                lines.drain(self.start..=self.end);
            }
        }
    }
}

pub(super) fn reject_overlaps(planned: &[PlannedEdit]) -> anyhow::Result<()> {
    let mut intervals = planned
        .iter()
        .map(PlannedEdit::interval)
        .collect::<Vec<_>>();
    intervals.sort();
    for pair in intervals.windows(2) {
        let (a_start, a_end) = pair[0];
        let (b_start, b_end) = pair[1];
        if a_end > b_start || (a_start == a_end && b_start == b_end && a_start == b_start) {
            anyhow::bail!("overlapping edits in one file are not allowed");
        }
    }
    Ok(())
}
