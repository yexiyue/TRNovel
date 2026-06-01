## ADDED Requirements

### Requirement: 网络书源目录的分卷标志（Phase 2）

`parse-book-source` 的目录解析 SHALL 支持卷标志：`RuleToc` MUST 新增可选的 `is_volume` 规则字段，解析出的 `Chapter` MUST 携带 `is_volume: bool`（默认 `false`）。该字段在反序列化旧书源时 MUST 以默认值兼容，不破坏既有书源。

#### Scenario: 书源声明卷规则

- **WHEN** 一个书源在 `ruleToc` 中提供了 `isVolume` 规则，且目录中存在卷条目
- **THEN** 对应条目被解析为 `is_volume = true`，其余为普通章节

#### Scenario: 旧书源无卷规则向后兼容

- **WHEN** 一个不含 `isVolume` 规则的既有书源被解析
- **THEN** 所有条目的 `is_volume` 为 `false`，行为与改造前一致

### Requirement: 网络小说复用折叠树目录（Phase 2）

网络小说 SHALL 复用与本地小说相同的「卷 → 章」折叠树目录。当书源目录包含卷条目时，网络小说目录 SHALL 按卷分组展示；不含卷时平铺展示。

#### Scenario: 含卷的网络小说分组展示

- **WHEN** 打开一本书源返回了卷条目的网络小说目录
- **THEN** 目录以卷为父节点分组，章为叶节点，交互与本地小说一致

#### Scenario: 不含卷的网络小说平铺展示

- **WHEN** 打开一本书源未返回卷条目的网络小说目录
- **THEN** 章节平铺展示，与现有网络小说目录体验一致
