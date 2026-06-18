## ADDED Requirements

### Requirement: Ambient 单例以进程级原子承载并跨页面持久

主题配置、TTS 模型句柄、浏览器验证提示等**无退出存档需求**的全局单例 SHALL 以进程级原子(`static Atom`)承载;任意页面 MUST 能订阅其当前值并在其变化时重渲染;其值 MUST 跨页面导航持久(不随单个页面卸载而丢失)。

#### Scenario: 主题变更全局生效

- **WHEN** 用户在主题设置页修改主题
- **THEN** 当前页与后续导航到的任意页面 MUST 立即反映新主题,无需重启

#### Scenario: TTS 模型句柄跨页面保留

- **WHEN** 用户在阅读页加载 TTS 模型后离开再返回
- **THEN** 已加载的模型句柄 MUST 仍可用,MUST NOT 因页面卸载而被重新加载

### Requirement: 浏览器验证提示可由非 UI 引擎代码触发

撞反爬挑战时,书源引擎(非 UI 代码路径)MUST 能直接写入浏览器验证提示状态以弹出验证模态,**不依赖**任何从 UI 组件穿线下来的状态句柄。

#### Scenario: 引擎撞挑战弹出验证模态

- **WHEN** `build_engine` 在后台请求中撞到需浏览器辅助验证的挑战
- **THEN** 它 MUST 能直接设置全局浏览器提示状态,UI MUST 据此弹出验证模态(授权/点击/通知)

#### Scenario: 取消信号在异步授权期间存活

- **WHEN** 浏览器提示为「点击验证」(内含取消信号),用户与之交互、后台 `authorize()` 仍在轮询取消信号
- **THEN** 写入新提示状态 MUST 采用替换语义,取消信号 MUST NOT 在 `authorize()` 仍引用它时被提前释放

### Requirement: 带退出存档的缓存必须保留析构存档语义

`History`、`BookSourceCache`、`TTSConfig` 等带 `Drop` 自动存档的缓存 MUST 由 App 持有于会触发 `Drop` 的存储中(随 App 卸载存档),MUST NOT 放入进程级 `static`(其析构永不运行,会丢失退出兜底存档)。

#### Scenario: 退出时兜底存档触发

- **WHEN** 用户正常退出应用(无显式保存动作)
- **THEN** 历史 / 书源 / TTS 配置 的 `Drop::save()` MUST 触发,最新状态 MUST 落盘

#### Scenario: App 生命周期内承载者不被中途重建

- **WHEN** 应用在运行中切换页面
- **THEN** 承载这些缓存的存储 MUST 全程存活,MUST NOT 因页面切换被重建而触发过早存档
