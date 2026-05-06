#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BenchContextMode {
    Cards,
    Ask,
    All,
}

impl BenchContextMode {
    pub(crate) fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "cards" => Ok(Self::Cards),
            "ask" => Ok(Self::Ask),
            "all" => Ok(Self::All),
            other => {
                anyhow::bail!("unknown bench context mode `{other}`; expected cards, ask, or all")
            }
        }
    }

    pub(crate) fn includes_raw_file(self) -> bool {
        matches!(self, Self::All)
    }

    pub(crate) fn includes_lexical(self) -> bool {
        matches!(self, Self::All)
    }

    pub(crate) fn includes_cards(self) -> bool {
        true
    }

    pub(crate) fn includes_ask(self) -> bool {
        matches!(self, Self::Ask | Self::All)
    }
}
