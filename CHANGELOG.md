## [0.10.0] - 2026-06-01

### 🚀 Features

- *(docs)* 添加关于使用 cargo-dist 发布 TRNovel 的详细文档
- 本地章节智能识别+分卷 & 书源引擎 v2 重写 + doctor 命令
- *(book-source)* 反爬支持——挑战检测 + 系统浏览器解挑战(cookie 烤箱)
- *(parse-book-source)* Schemars 从类型自动生成 JSON Schema + 防漂移测试
- *(skills)* Booksource-generator —— 据网站自动生成书源 + 真实站点评测

### 🐛 Bug Fixes

- 消除 clippy collapsible_match 警告 (新版 stable lint)
- *(docs)* 声明 pnpm onlyBuiltDependencies 修复 GitHub Pages 部署
- *(docs)* 钉定 pnpm@10.32.1 修复 latest pnpm 不识别 onlyBuiltDependencies 的部署失败

### 📚 Documentation

- 增加 CLAUDE.md 与 OpenSpec 设计(toc-rule-engine、ai-friendly-book-source)
- *(browser-fetcher)* OpenSpec 设计(proposal/design/specs/tasks)+ 反爬浏览器辅助参考文档
- Refresh README and add evolution blog

### ⚙️ Miscellaneous Tasks

- Gitignore 忽略本地工具目录 .claude / .obsidian
## [trnovel-v0.9.0] - 2026-05-28

### 🐛 Bug Fixes

- Windows 动态链接 C 运行时 (msvc-crt-static=false)

### ⚙️ Miscellaneous Tasks

- 用 cargo-dist 接管二进制发布与 npm/homebrew 分发
- 移除不再使用的 Cross.toml (cargo-dist 原生构建，不走 cross)
- Release
## [0.8.4] - 2025-11-30

### 🐛 Bug Fixes

- 修复显示隐藏标题未保存

### ⚙️ Miscellaneous Tasks

- Release
## [0.8.3] - 2025-11-30

### 🐛 Bug Fixes

- *(cd)* 调整Linux环境下的OpenSSL配置和依赖安装

### 💼 Other

- *(cd)* 切换到 cross 工具进行Linux跨平台构建
- *(Cross.toml)* 更新依赖安装命令
- *(Cross.toml)* 更新交叉编译依赖安装命令
- *(Cross.toml)* 添加环境变量传递配置以支持预编译库下载
- *(cd)* 配置 OpenSSL 环境变量以支持静态链接
- *(cd)* 调整CI构建流程中的依赖安装和环境配置

### 🎨 Styling

- Clippy
- Fmt

### ⚙️ Miscellaneous Tasks

- *(cd)* 调整 Linux 平台构建流程以支持 Zig 编译器
- *(cd)* 调整 Linux 环境下的依赖安装逻辑
- *(cd)* 更新Linux依赖安装命令
- *(cd)* 更新linux环境下的openssl依赖配置
- Release
## [0.8.2] - 2025-11-13

### 🐛 Bug Fixes

- 修复下载任务二次下载进度不统一问题 Close(#46)

### 🎨 Styling

- Clippy

### ⚙️ Miscellaneous Tasks

- Release
## [0.8.1] - 2025-11-12

### 🐛 Bug Fixes

- 修复最后一段丢失bug (Close #44)

### 📚 Documentation

- Update home

### ⚙️ Miscellaneous Tasks

- Release
## [0.8.0] - 2025-10-31

### 🚀 Features

- 添加重置主题功能

### 🐛 Bug Fixes

- 修复历史记录跳转到本地小说异常
- 修复某些界面没有使用主题情况

### 📚 Documentation

- 添加网络小说文档
- README

### ⚙️ Miscellaneous Tasks

- Minor version
- Release
## [0.7.9] - 2025-10-30

### 🚀 Features

- 添加主题设置界面

### 🐛 Bug Fixes

- 修复全局快捷键冲突 Close(#40)

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.8] - 2025-10-29

### 🐛 Bug Fixes

- 解决输入按键冲突

### 📚 Documentation

- 添加听书模式文档

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.7] - 2025-10-29

### 🐛 Bug Fixes

- 修复前进后退按键冲突

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.6] - 2025-10-29

### 🚀 Features

- *(docs)* Init
- 添加文档

### 🐛 Bug Fixes

- 修复排除 crate 列表的添加逻辑
- Docs add base
- 修复快捷键提示

### 📚 Documentation

- 添加阅读文档

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.5] - 2025-10-26

### 🐛 Bug Fixes

- 修复快速模式打开失败
- 更新 ratatui-kit 版本并修复 tts 模态框交互逻辑

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.4] - 2025-10-24

### 🚀 Features

- 添加 tts queue
- 支持文本高亮
- *(novel-tts)* 实现文本高亮与精确位置追踪
- 实现听书自动播放下一章节功能
- 添加防抖效果钩子并优化声音选择功能

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.3] - 2025-10-21

### 🚀 Features

- 支持切换声音
- *(novel-tts)* 文本预处理

### 🐛 Bug Fixes

- 修复发布流程识别不到changelog问题
- 修复本地路径持久化 Close #36

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.2] - 2025-10-20

### 🐛 Bug Fixes

- 更新publish workflow

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.1] - 2025-10-20

### 🚀 Features

- 添加tts快捷键信息

### 🐛 Bug Fixes

- Ci
- Publish with log

### ⚙️ Miscellaneous Tasks

- Release
## [0.7.0] - 2025-10-19

### 🚀 Features

- Use dirs create
- Add novel-tts
- 支持流式小说转语音
- 添加听书设置
- 支持播放暂停

### 🐛 Bug Fixes

- Clippy
- Clippy
- Ci
- Publish.yaml
- Publish.yaml

### 💼 Other

- Update publish.yaml
- Changelog

### ⚙️ Miscellaneous Tasks

- Release
- Test
- Test
- Test
- Test
## [0.6.2] - 2025-10-12

### 🚀 Features

- 添加快捷键信息

### ⚙️ Miscellaneous Tasks

- Release
## [0.6.1] - 2025-10-12

### 🚀 Features

- 优化整体交互逻辑

### 💼 Other

- Clippy

### ⚙️ Miscellaneous Tasks

- Release
## [0.6.0] - 2025-10-12

### 💼 Other

- Modify publish.yaml
- Modify publish.yaml
- Modify publish.yaml
- Modify publish.yaml
- Remove dependencies
- Update ci.yml
- Cd.yml
- Cd.yml
- Update ci.yml
- Update ci.yml
## [parse-book-source-v0.2.0] - 2025-10-11

### 🚀 Features

- 添加use_theme_config hook
- 重构搜索输入组件
- 完成select组件
- Add file select component
- 添加file select组件
- 添加list select组件
- 搜索小说文件
- 重构小说阅读页面
- 添加路由导航
- 重构导入书源页面
- 重构书源管理
- 重构书籍列表
- 优化find book页面
- 重构书籍详情页面

### 🐛 Bug Fixes

- 删除旧架构
- 优化章节目录选择

### 💼 Other

- Home page
- Ci/cd

### 🎨 Styling

- Clippy

### ⚙️ Miscellaneous Tasks

- Release
## [0.5.4] - 2025-02-05

### 🐛 Bug Fixes

- Confirm style

### ⚙️ Miscellaneous Tasks

- Release
## [0.5.3] - 2025-01-24

### 🚀 Features

- 支持鼠标点击翻页

### ⚙️ Miscellaneous Tasks

- Release
## [0.5.2] - 2025-01-24

### 🚀 Features

- 支持滚轮翻页

### ⚙️ Miscellaneous Tasks

- Release
## [0.5.1] - 2025-01-17

### ⚙️ Miscellaneous Tasks

- Release
## [0.5.0] - 2025-01-15

### 📚 Documentation

- Update readme

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.9] - 2025-01-14

### 🚀 Features

- 主题设置
- Add info
- Update todo

### 💼 Other

- ThemeSetting
- Fmt

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.8] - 2025-01-10

### 💼 Other

- 按键冲突

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.7] - 2025-01-09

### 🚀 Features

- 优化选择章节
- Theme setting

### 🐛 Bug Fixes

- Test

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.6] - 2025-01-07

### 🚀 Features

- 本地小说文件路径优化，默认打开上一次的文件夹

### 🐛 Bug Fixes

- 修复章节搜索跳转问题

### 💼 Other

- Release.sh

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.5] - 2025-01-06

### 🚀 Features

- 支持cookie

### 🐛 Bug Fixes

- Test

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.4] - 2025-01-06

### 🚀 Features

- Opt

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.3] - 2025-01-05

### 🚀 Features

- 支持html规则

### 💼 Other

- 重构网络小说，支持最新的解析器

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.2] - 2024-12-25

### 💼 Other

- 修改set_list

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.1] - 2024-12-25

### 💼 Other

- 重构read novel，将读取文件放入初始化

### ⚙️ Miscellaneous Tasks

- Release
## [0.4.0] - 2024-12-23

### 🐛 Bug Fixes

- 修复大文件无目录时卡顿问题

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.7] - 2024-12-22

### 🚀 Features

- Quick start

### 💼 Other

- Clippy

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.6] - 2024-12-21

### 🚀 Features

- Update release
- Update release

### 💼 Other

- Clippy

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.5] - 2024-12-21

### 🚀 Features

- 完善快捷键信息

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.4] - 2024-12-21

### 💼 Other

- 重构本地小说，支持输入路径
- Clippy

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.3] - 2024-12-21

### 🚀 Features

- 另起仓库发布npm
- Welcome

### 🧪 Testing

- Npm ci
- Npm ci
- Npm ci
- Npm ci
- Npm ci
- Npm ci

### ⚙️ Miscellaneous Tasks

- Release
- Release
- Release
## [0.3.2] - 2024-12-19

### 🚀 Features

- Update todo
- 支持npm

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.1] - 2024-12-18

### 🚀 Features

- 优化novel trait
- 优化书籍缓存
- 导入网络url
- 书源管理页面
- 优化导入列表样式
- 历史页面
- 优化历史记录UI
- 移除本地的历史记录

### 💼 Other

- 优化章节选择
- 修复空书源页面
- 优化文案

### ⚙️ Miscellaneous Tasks

- Release
## [parse-book-source-v0.1.1] - 2024-12-16

### 🚀 Features

- 优化Page 和 Router Trait
- Network explore
- 书籍详情页面
- Read content
- 重构阅读页面
- Parse-book-source支持 serde

### ⚙️ Miscellaneous Tasks

- Release
## [0.3.0] - 2024-12-13

### 🚀 Features

- Update todo
- Add select explore
- 重新设计架构，支持自定义消息
- 重构支持消息，为网络小说铺垫
- Update todo
- Add router life cycle

### 💼 Other

- Remove test bin
## [0.2.2] - 2024-12-05

### 🚀 Features

- 添加trn别名
## [0.2.1] - 2024-12-04

### 💼 Other

- 修复确认框样式
## [0.2.0] - 2024-12-03

### 🚀 Features

- Update todo
- 支持没有章节的文本
- Add delete history
- Add table cell
- Add key shortcut info

### 🐛 Bug Fixes

- 修复高度问题

### 💼 Other

- Fmt
## [0.1.1] - 2024-11-30

### 🐛 Bug Fixes

- 区分警告和错误
## [0.1.0] - 2024-11-29

### 🚀 Features

- Add todo.md
- 添加Routes管理页面
- 重构事件流
- 优化警告样式
- Add state
- 阅读模式优化
- Ci
- Add README

### 💼 Other

- 读取历史记录文件异常
- SelectNovel
- Fmt
- Clippy

### 🧪 Testing

- Ci
