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

产物位于 `src-tauri/target/release/bundle/macos/CleanDisk.app`。首版不使用 Apple Developer 证书、不公证、不生成 DMG。

本次验收构建另存为 `artifacts/CleanDisk.app`（该目录不纳入 Git）。

## Homebrew 安装

Apple Silicon Mac 可以通过个人 Tap 安装最新版：

```bash
brew install --cask onewesong/tap/cleandisk
```

应用会安装到 `/Applications/CleanDisk.app`。当前发布包使用 ad-hoc 签名且未经过 Apple 公证，首次打开时 macOS 可能要求在“系统设置 > 隐私与安全性”中确认。

卸载：

```bash
brew uninstall --cask cleandisk
```

## CI 与发布

GitHub Actions 会在推送到 `main` 或向 `main` 提交 Pull Request 时运行前端测试、前端构建和 Rust 测试。

推送格式为 `v<semver>` 的标签会自动发布仅支持 Apple Silicon 的 macOS arm64 应用，例如 `v0.2.0`。发布前必须把以下三个文件中的版本号同步为不带 `v` 的相同版本：

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

发布示例：

```bash
# 先将三个版本文件都更新为 0.2.0，然后提交并推送
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore: release v0.2.0"
git push origin main
git tag v0.2.0
git push origin v0.2.0
```

CI 会校验标签与三个版本文件完全一致，随后创建公开的 GitHub Release，并上传 `.app` 压缩产物。发布包使用无需证书的 ad-hoc 签名，但没有经过 Apple 公证；首次打开时 macOS 仍可能显示“未知开发者”提示，需要在“系统设置 > 隐私与安全性”中确认打开。

## 结构

- `src-tauri/src/plugins.rs`：固定插件协议与内置扫描规则。
- `src-tauri/src/scanner.rs`：顺序扫描、去重、排序、快照与复核。
- `src-tauri/src/cleaner.rs`：固定清理主流程，可注入假废纸篓后端测试。
- `src-tauri/src/commands.rs`：Tauri commands、单任务状态与 Channel 事件。
- `src/`：简体中文单窗口界面、筛选、确认和前后空间对比。

现有 `clean-disk-space` Python Skill 仅作为行为参考，与本应用独立维护。
