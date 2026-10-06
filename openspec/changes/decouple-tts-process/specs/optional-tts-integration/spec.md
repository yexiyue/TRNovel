# Spec Delta

## Purpose

定义阅读器编译时可选的听书交互组件及其与独立程序的职责关系，并规定基础阅读包和听书包的依赖、安装与兼容行为，使阅读功能独立于音频推理和播放环境。

## ADDED Requirements

### Requirement: 编译时可选听书组件
阅读器 SHALL 提供编译时可选听书 UI。基础构建 SHALL 隐藏听书入口、面板、帮助与活动键位，不读取或写入听书配置、模型及检查点。已有 keybindings 中的听书项 SHALL 安全忽略而不阻塞阅读。

#### Scenario: 基础阅读构建
- **WHEN** 用户运行未编译听书组件的阅读器
- **THEN** 所有阅读功能和阅读设置可用，界面无听书入口，原有听书数据保持不变

#### Scenario: 共享键位文件
- **WHEN** 基础版使用听书版留下的键位配置
- **THEN** 听书键位不成为活动操作，也不使配置加载失败

### Requirement: 阅读器依赖隔离
两种阅读器构建 SHALL 均不链接由听书引入的模型推理或音频播放依赖；启用听书 UI SHALL 只通过进程协议使用独立程序。基础阅读运行 SHALL 不要求听书动态库或音频设备。

#### Scenario: 检查应用依赖与运行
- **WHEN** 单独检查基础及启用听书 UI 的应用依赖树并在无听书运行库环境启动
- **THEN** 应用不包含听书引入的 rodio、kokoro-tts 和 ort，普通阅读正常运行

### Requirement: 章节与自动续章归阅读器
TRNovel SHALL 获取章节正文、管理阅读进度并决定自动续章。听书程序 SHALL 只处理收到的正文并返回原文位置；只有当前有效会话的正常完成 SHALL 在 auto_play 启用时触发下一章。跨启动恢复 MUST NOT 自动开始播放。

#### Scenario: 正常自动续章
- **WHEN** 当前会话正常播放完成且 auto_play 启用、存在下一章
- **THEN** 阅读器获取并提交下一章，不要求听书程序了解书源

#### Scenario: 失败或全书结束
- **WHEN** 会话失败、用户取消或已无下一章
- **THEN** 不继续获取后续章节，UI 正确显示故障、停止或已完成状态

### Requirement: 进度与搜索高亮协调
听书 UI SHALL 根据当前正文的有效播放范围显示高亮，搜索命中样式 SHALL 保持优先，搜索清除后 SHALL 恢复当前听书高亮。

#### Scenario: 搜索重叠
- **WHEN** 搜索命中与播放范围重叠后用户清除搜索
- **THEN** 重叠期间搜索样式优先，清除后仍显示当前有效播放范围

### Requirement: 基础与听书发行包
发行 SHALL 提供明确可选的基础阅读包与听书包。基础包 SHALL 包含不带听书 UI 的阅读器；听书包 SHALL 同目录安装启用听书 UI 的阅读器及兼容听书程序，不附带自动下载的模型。安装渠道 SHALL 说明并支持明确的变体选择。

#### Scenario: 安装听书包
- **WHEN** 用户通过支持的渠道安装听书包
- **THEN** trnovel/trn 可找到同目录听书程序，首次启用才准备模型

#### Scenario: 安装基础包
- **WHEN** 用户明确选择基础包
- **THEN** 安装不包含听书程序，不要求 ORT/ALSA 等听书运行依赖

#### Scenario: 已支持的平台
- **WHEN** 为 Windows MSVC、Linux GNU、Apple Silicon、Intel Mac 和 ARM64 musl 发布制品
- **THEN** 两种包均按平台验证，musl 听书共享库要求仅适用于听书程序并明确说明
