## What changed

<!-- The model, not the diff. If this is a bug fix, name the invariant that was
     violated rather than the symptom. -->

## Gates

<!-- Which gate covers this? A rule with no gate is a rule that will be
     violated silently, so a contract change and its gate land together. -->

- [ ] `cargo test --workspace` is green
- [ ] a contract change, if any, cites a gate that exists
- [ ] a moved size or cost bound, if any, states its reason on the line above

## Review

See `AGENTS.md`. Defects are rejections; performance shape is a question, and
"not on a hot path" is an accepted answer.
