# Proposal: chapter-content-search

## Why

Issue #77 requests searching body text while reading. The existing search filters chapter names only.

## What Changes

Add literal current-chapter search with a one-row input, submit/cancel, match highlighting, cyclic navigation and configurable reader actions. Share plain-text wrapping and byte-coordinate mapping between rendering and navigation; TTS continues and search styles win on overlap.

## Impact

Local and network novels use the same loaded chapter content. No additional fetching, dependencies, persisted search state or reading-progress migration. Whole-book search is deferred.
