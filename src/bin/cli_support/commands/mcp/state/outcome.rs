pub(crate) fn response_has_error(output: &str) -> bool {
    response_error_code(output).is_some()
}

pub(crate) fn response_error_code(output: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .and_then(|value| {
            let ok_false = value
                .get("ok")
                .and_then(|ok| ok.as_bool())
                .map(|ok| !ok)
                .unwrap_or(false);
            ok_false
                .then(|| value.pointer("/error/code")?.as_str().map(str::to_string))
                .flatten()
        })
}

pub(crate) fn saved_context_metric(tool: &str, errored: bool) -> Option<&'static str> {
    if errored {
        return None;
    }
    match tool {
        "synrepo_note_add" => Some("note_add"),
        "synrepo_note_link" => Some("note_link"),
        "synrepo_note_supersede" => Some("note_supersede"),
        "synrepo_note_forget" => Some("note_forget"),
        "synrepo_note_verify" => Some("note_verify"),
        _ => None,
    }
}
