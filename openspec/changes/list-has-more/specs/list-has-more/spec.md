## ADDED Requirements

### Requirement: explore/search 翻页边界(has_more)
`explore`/`search` 配置可选 `hasMore` 规则时,系统 SHALL 在单页取页后对该页响应用 `hasMore` 规则求值,得「是否还有下一页」,并随书列表一并返回(`has_more: Option<bool>`)——求值结果非空且非 `false`/`0` 视为还有下一页。无 `hasMore` 规则时返回 `None`(不提供边界,现状)。该求值 MUST 复用同一页响应,不额外发请求。

#### Scenario: 从 API 响应读 has_more
- **WHEN** `explore` 配 `hasMore: {via:json, select:"$.data.has_more"}`,拦到的 `book_list` JSON 含 `has_more=true`
- **THEN** 返回的 `has_more` 为 `Some(true)`,UI 可继续翻下一页

#### Scenario: 到头边界
- **WHEN** 某页响应 `has_more=false`
- **THEN** 返回 `has_more` 为 `Some(false)`,UI 不再允许翻下一页(到头停)

#### Scenario: 无 hasMore 规则不提供边界
- **WHEN** `explore`/`search` 未配 `hasMore`
- **THEN** 返回 `has_more` 为 `None`,UI 不限制翻页(与本 change 之前一致)
