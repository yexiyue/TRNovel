# 设计

## D1 落盘放在页面，而不是根组件

根组件在启动早期就开始渲染，此时 `READER_DISPLAY` 还是默认值，异步加载尚未完成；
在根上监听 atom 会把默认值写回磁盘。放在页面级、以进入页面时的 atom 值作快照，
既能覆盖「本页改动」，又不会把加载前的默认值当成用户修改。

## D2 防抖 + 卸载兜底，两份而不是抽公共 hook

全局设置页与阅读页的落盘写法必须包含「写失败不推进快照」与「卸载时同步补写」两个细节。
把它抽成 hook 需要同时解决 `use_state` 快照与 `use_on_drop` 的借用，收益不抵风险；
两处各自保留约 20 行、注释写明原因，后续任何一处调整都要同步另一处。

## D3 测试分层

- 字段级：`every_reader_display_field_survives_serialization` 与
  `every_appearance_field_survives_serialization` 给每个字段非默认值，防止新增字段漏
  `serde(default)` 或在序列化时丢掉。
- 磁盘级：`every_reader_and_appearance_field_survives_a_disk_round_trip` 经
  `ConfigStore::update/load` 走真实 TOML 文件与 `normalize()`。
- 条目级：`setting_rows_cover_every_item_exactly_once` 防止浮层条目表漏项/重项。
