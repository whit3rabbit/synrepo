## No spec changes required

The existing specs already describe the intended behavior:

- **graph/spec.md**: Requirement "Define graph and git-intelligence boundary" includes scenario "Attach co-change evidence to a file" with `git_observed` authority.
- **git-intelligence/spec.md**: Requirement "Define git-intelligence outputs" lists co-change partners as a defined output.
- **cards/spec.md**: Budget tier requirement at `normal` budget includes "co-change partners" in the progressive disclosure contract. FileCard git intelligence surfacing includes "co-change partners" in the payload.

This change closes the implementation gap without modifying spec requirements.
