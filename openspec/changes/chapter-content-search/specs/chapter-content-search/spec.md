## ADDED Requirements

### Requirement: Search only the loaded chapter
The reader SHALL perform case-sensitive, non-overlapping literal matching in each original paragraph. Empty submissions clear results. Search SHALL NOT fetch other chapters or interpret regular expressions.

#### Scenario: Submit a keyword
- **WHEN** the user opens search with `s`, types a keyword and presses Enter
- **THEN** the input closes, all matches highlight, the first match becomes visible and the result count is displayed

#### Scenario: Navigate and clear
- **WHEN** the user presses `n` or `N`
- **THEN** selection cycles forward or backward within the chapter
- **WHEN** the user presses Esc outside editing
- **THEN** results clear without changing the reading position

### Requirement: Input is exclusive and transactional
The one-row input SHALL use an exclusive input layer. Draft editing SHALL NOT change committed matches or reading position. Enter submits; Esc cancels while retaining the previous search. Chinese text, editing and paste SHALL be supported.

#### Scenario: Type shell or reader keys
- **WHEN** a search draft contains q, b, g or reader shortcut characters
- **THEN** they edit the draft without triggering navigation, quitting or reader actions

### Requirement: Lifecycle and layout remain safe
Chapter changes and leaving reading mode SHALL clear search, including chapters with identical content. Content updates SHALL recompute active results. Search SHALL be unavailable during loading or settings/help overlays.

#### Scenario: Resize or change paragraph spacing
- **WHEN** layout changes with an active result
- **THEN** its original byte range maps to the new display rows and remains visible
- **AND** ordinary manual scrolling does not force a return to that result

### Requirement: Highlighting preserves text and TTS
Highlight spans SHALL NOT change wrapping, indentation or paragraph spacing. Current search matches SHALL be bold; other matches use the theme accent. TTS SHALL continue and overlapping search styles take precedence.

#### Scenario: Clear search during speech
- **WHEN** the user clears search while speech is playing
- **THEN** speech continues and its normal highlight is restored

### Requirement: Reader search actions are configurable
The reader SHALL expose search_content, next_search_match, prev_search_match and clear_content_search, defaulting to `s`, `n`, `N` and `esc`. Help and hints SHALL display effective bindings and distinguish n from N.
