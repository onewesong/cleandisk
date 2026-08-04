# CleanDisk

CleanDisk 是一个仅面向 macOS arm64 的本地磁盘清理应用。前端使用 React + TypeScript，扫描与安全校验全部由 Rust 完成；应用不联网、不永久删除文件，也不依赖 Python。

界面采用四步向导：选择扫描类别、查看实时扫描进度、审查并选择候选、查看清理前后空间对比。扫描类别默认全选，扫描结束前不会展示或允许选择不完整的候选结果。

## 安全模型

- 候选项默认全部不勾选，必须在摘要弹窗中由用户确认。
- 前端只能提交扫描会话内的候选 ID，不能提交任意路径。
- 清理前重新核对设备号、inode、mode、容量与递归元数据摘要。
- 不跟随符号链接，只允许当前用户目录且与废纸篓同一文件系统的路径。
- 使用 macOS 原生废纸篓机制，失败时跳过，不回退为永久删除。
- 项目依赖与未被 VS Code 标记 obsolete 的旧扩展统一归为“需审查”。

## 开发

```bash
npm install
npm run tauri dev
```

验证与构建：

```bash
npm test
npm run build
cd src-tauri && cargo test
npm run tauri build -- --bundles app
```

产物位于 `src-tauri/target/release/bundle/macos/CleanDisk.app`。首版不签名、不公证、不生成 DMG。

本次验收构建另存为 `artifacts/CleanDisk.app`（该目录不纳入 Git）。

## 结构

- `src-tauri/src/plugins.rs`：固定插件协议与内置扫描规则。
- `src-tauri/src/scanner.rs`：顺序扫描、去重、排序、快照与复核。
- `src-tauri/src/cleaner.rs`：固定清理主流程，可注入假废纸篓后端测试。
- `src-tauri/src/commands.rs`：Tauri commands、单任务状态与 Channel 事件。
- `src/`：简体中文单窗口界面、筛选、确认和前后空间对比。

现有 `clean-disk-space` Python Skill 仅作为行为参考，与本应用独立维护。
