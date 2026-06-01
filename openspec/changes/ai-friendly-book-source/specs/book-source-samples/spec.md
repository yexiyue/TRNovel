## ADDED Requirements

### Requirement: 书源携带黄金样例

书源 SHALL 可选携带 `samples`:每个样例含一个 `bookUrl` 与一组期望不变量(如 `name`、`minChapters`、`volumes`、`minContentChars`)。样例用于验证该书源规则是否正确工作。

#### Scenario: 样例声明期望

- **WHEN** 书源 `samples` 含 `{ "bookUrl": "/novel/guzhenren.html", "expect": { "volumes": 8, "minChapters": 2000 } }`
- **THEN** 该期望可被验证流程读取并据以断言

### Requirement: 可执行不变量校验

系统 SHALL 提供 `verify`:对样例书跑完整流程并断言各阶段可执行不变量——书信息 `name` 非空;目录章节数达标且章节 URL 合法;正文字数 ≥ 阈值且非"挑战/空"页;若声明则卷数匹配。`verify` MUST 返回逐项通过/失败结果。

#### Scenario: 规则正确时校验通过

- **WHEN** 书源规则正确,对样例运行 `verify`
- **THEN** 所有不变量通过

#### Scenario: 规则错误时给出结构化失败反馈

- **WHEN** 某字段规则失效(如目录选择器选不到节点)
- **THEN** `verify` 失败,并返回「失败的断言名 + 期望值 + 实际值 + 该规则当前命中的 HTML 片段」结构化反馈(供人或 AI 据以修复)

### Requirement: 运行期自愈

当某规则配置了 `firstOf` 候选时,运行期某候选失效 SHALL 自动退到下一候选。若全部候选均无法满足该字段的基本不变量,系统 SHALL 将该书源标记为需修复,而非静默返回错误数据。

#### Scenario: 候选选择器自动退化

- **WHEN** `firstOf` 首个选择器因站点改版失效,次选仍有效
- **THEN** 运行期自动使用次选,抓取继续成功

#### Scenario: 全部候选失效时显式失败

- **WHEN** 某字段所有 `firstOf` 候选都选不到内容,违反该字段不变量
- **THEN** 系统报告该书源失效(提示换源/重生成),不把空/错内容当正常结果
