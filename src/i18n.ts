export const zh={
 appName:"CleanDisk",tagline:"安全地找回 Mac 空间",settings:"项目目录",availableSpace:"可用空间",itemUnit:"项",locale:"zh-CN",
 steps:["选择扫描项","实时扫描","选择清理项","完成"],
 scopeTitle:"选择要扫描的内容",scopeHint:"扫描是只读操作，不会移动或删除任何文件。",selectAll:"全选",clearAll:"清空",startScan:"开始扫描",selectedCategories:"已选择扫描类别",manageRoots:"管理项目目录",projectRootCount:(count:number)=>`当前配置 ${count} 个项目根目录`,
 category:{"application-caches":"应用缓存","developer-caches":"开发工具缓存","project-dependencies":"项目依赖",logs:"日志","crash-reports":"崩溃报告","download-leftovers":"下载残留",other:"其他"},
 categoryDescription:{"application-caches":"超过 30 天未变化、可由应用重新生成的缓存","developer-caches":"开发工具缓存与重复的 VS Code 扩展","project-dependencies":"项目中的 node_modules、.venv 和 venv","logs":"超过 30 天未变化的用户日志","crash-reports":"较旧的应用崩溃与诊断报告","download-leftovers":"较旧的 DMG、PKG 和 ZIP 下载残留","other":"其他插件发现的项目"},
 scanningTitle:"正在扫描所选内容",scanningHint:"候选结果会在全部扫描完成后统一展示。",cancelScan:"取消扫描",currentCategory:"当前类别",currentPlugin:"扫描插件",currentPath:"正在检查",visited:"已检查路径",found:"已发现候选",accumulated:"累计容量",completedPlugins:"已完成阶段",waiting:"正在准备扫描…",
 scanCancelled:"扫描已取消，已保留你的类别选择。",scanWarning:"扫描警告：",retry:"重新扫描",chooseAgain:"重新选择扫描项",
 resultsTitle:"扫描结果",resultsHint:"候选项默认不勾选，请按风险和原因逐项审查。",search:"搜索路径或原因",all:"全部",low:"低风险",review:"需审查",select:"选择",selectGroup:"全选分组",unselectGroup:"取消全选",selected:"已选",move:"移入废纸篓",
 confirmTitle:"确认移入废纸篓",confirm:"确认移动",close:"取消",confirmSummary:(low:number,review:number)=>`低风险 ${low} 项，需审查 ${review} 项。内容将移入系统废纸篓，不会永久删除。`,dependencyWarning:"包含项目依赖，之后需要通过包管理器重新安装。",
 cleaning:"正在移入废纸篓",cleaningHint:"清理开始后不能取消，请保持应用开启。",rescanning:"正在复扫剩余候选…",movingProgress:(done:number,total:number)=>`正在移动 ${done}/${total}`,
 completeTitle:"清理完成",completeHint:"文件已移入系统废纸篓，下面是本次操作的空间对比。",moved:"已移动",failed:"失败",freeBefore:"清理前可用",freeAfter:"清理后可用",freeDelta:"可用空间变化",trashReclaimable:"废纸篓可回收",trashNote:"移入废纸篓通常不会立即释放磁盘空间；永久释放需要你另行手动清空废纸篓。",viewRemaining:"查看剩余候选",scanAgain:"重新扫描",
 settingsTitle:"项目扫描目录",settingsHint:"仅扫描这些目录中的 node_modules、.venv 和 venv。",remove:"移除",addRoot:"＋ 添加目录",done:"完成",closeBlocked:"清理正在进行，请等待完成后再退出。",
} as const;
