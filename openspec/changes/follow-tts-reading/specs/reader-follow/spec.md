## ADDED Requirements

### Requirement: Reader follows actual speech positions
The reader SHALL default to following actual played text and SHALL keep manual browsing under user control.

#### Scenario: Played text approaches the bottom
- **WHEN** the current played range starts in the bottom two rows or outside the viewport
- **THEN** the reader scrolls its start to roughly the upper third within legal bounds

#### Scenario: Manual browsing and explicit recovery
- **WHEN** the user scrolls, pages or searches
- **THEN** follow suspends until FollowPlayback is invoked, which enables follow and returns to the reliable current range

#### Scenario: Waiting or obscured content
- **WHEN** playback is paused or buffering, or search/modal content is active
- **THEN** automatic scrolling does not occur

#### Scenario: Chapter continuation and book replacement
- **WHEN** playback continues automatically into the next chapter
- **THEN** the follow suspension state is retained
- **WHEN** another book opens
- **THEN** the temporary suspension resets according to saved preferences

### Requirement: Follow controls are reader preferences
The reader SHALL persist the setting with legacy default true, expose remappable recovery keys, and hide controls in builds without TTS.

#### Scenario: Legacy configuration
- **WHEN** followTts is absent from stored display preferences
- **THEN** following is enabled
