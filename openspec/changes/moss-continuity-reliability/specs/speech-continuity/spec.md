## ADDED Requirements

### Requirement: Source-preserving contextual speech
MOSS SHALL merge soft line breaks within its 8-second target / 12-second ceiling, 50-token and 60-CJK budgets, preserve source UTF-8 ranges and keep blank lines, headings and decorations as hard boundaries.

#### Scenario: Wrapped prose
- **WHEN** adjacent short prose lines have one newline between them
- **THEN** they may share one synthesis request and the break does not imply paragraph-tail silence

#### Scenario: Hard boundaries
- **WHEN** prose encounters a blank line, heading or separator
- **THEN** blocks do not merge across it, headings are spoken independently and decorations are omitted

### Requirement: Honest synthesis termination
MOSS SHALL report structured termination diagnostics. A failed stream SHALL stop the session without submitting its completion or advancing to following text, and SHALL NOT automatically replay prior output.

#### Scenario: Frame ceiling
- **WHEN** generation reaches its ceiling without EOS
- **THEN** it reports frame-limit termination and no successful End

#### Scenario: Normal EOS
- **WHEN** the model emits EOS
- **THEN** diagnostics record it without asserting that audio covers all input words
