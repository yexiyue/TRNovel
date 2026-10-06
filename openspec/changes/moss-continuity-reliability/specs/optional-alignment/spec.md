## ADDED Requirements

### Requirement: Alignment is explicit
The program SHALL default alignment_enabled to false, including old configurations, and SHALL not prepare, download, load or calibrate alignment resources when disabled.

#### Scenario: Default preparation
- **WHEN** preparing a default listening configuration
- **THEN** only synthesis resources are prepared and highlighting is block based

#### Scenario: Preference changes
- **WHEN** sentence alignment is toggled
- **THEN** current playback is stopped, reliable progress is preserved and the user must restart

#### Scenario: UI setting
- **WHEN** sentence alignment is disabled
- **THEN** the alignment device row is hidden and the enable switch remains available
