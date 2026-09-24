# NCM Dump GUI

网易云音乐 `.ncm` 文件转换器（mp3 / flac），Tauri 2 + Rust 实现，界面风格参考 [taurusxin/ncmdump-gui](https://git.taurusxin.com/taurusxin/ncmdump-gui)，核心算法移植自 [taurusxin/ncmdump](https://github.com/taurusxin/ncmdump)

## 功能

### 解密核心
- AES-128-ECB + RC4 变种 keybox 流式解密，支持全版本 ncm（含 3.x 无内嵌封面）
- ID3 头识别 mp3 / fLaC 识别 flac，分块流式写出，内存占用低
- rayon 并行转换，逐文件进度事件（等待 / 转换中 / 成功 / 跳过 / 失败+原因）

### 元数据管线（核心特色）
- 全字段解析 ncm 内嵌元数据（含 musicId / albumId / albumPic 直链）
- 写入完整标签：标题、歌手（多歌手 ` / ` 连接）、专辑、**音轨号、碟号、年份、专辑歌手**
- 封面三级策略：内嵌封面 → 元数据 albumPic URL 直下（无需 API）→ `/album` 接口高清兜底
- **歌词嵌入**：lrc 原文 + tlyric 翻译 → mp3 USLT / flac LYRICS
- 网络补全：`/song/detail` + `/album` 匿名接口补全音轨号/碟号/发行年/专辑歌手/厂牌（可开关，失败静默降级）
- 无元数据文件按文件名兜底，任何 ncm 都能出结果

### 个性化
- 设置全量持久化：`%APPDATA%\com.animinzes.ncm-dump-gui\config.json`（人类可读、原子写、缺字段自动回默认）
- 网易云目录自动定位：注册表探测 + 常见位置 + 历史目录，工具栏「快速导入」一键选择
- 命名模板：`{artist} {title} {album} {track} {disc} {year} {bitrate}` 占位符，Windows 非法字符自动清洗
- 音乐库归档：`库/{歌手}/{专辑}/` 目录结构自动入库，同名冲突 跳过/覆盖/另存 三种策略
- 转换后源文件：保留 / 移入回收站 / 删除
- 记忆窗口尺寸与上次目录

### 界面
浅色简洁中文界面：工具栏（添加文件 / 添加目录 / 清除列表 / 开始处理）、保存位置单选（源目录 / 自定义 / 音乐库归档）、可折叠设置区、状态/文件/操作三列列表、整窗拖放、着色状态徽标。

## 开发

```shell
pnpm install        # 前端依赖
pnpm tauri dev      # 开发模式
pnpm tauri build    # 出 release exe
cargo test          # 后端单测（解密 / 标签 / 配置 / 模板）
```

手动验证工具（`src-tauri/examples/`，输出仅写临时目录）：

```shell
cargo run --example verify_network   # 用 test.ncm 的 musicId 实测网易 API 字段
cargo run --example verify_e2e       # 网络全开的完整转换 + 标签回读断言
cargo run --example extract_meta     # 查看 ncm 内嵌元数据
```

> crates.io 依赖走 `src-tauri/.cargo/config.toml` 配置的 rsproxy.cn 镜像（项目级，不动全局）。

## 项目结构

```
src-tauri/src/
├── ncm/          # 解密核心（keys / crypt / metadata）
├── metadata/     # lofty 标签写入（ID3v2 / Vorbis + 封面）
├── netease/      # 网易 API 客户端（song/detail / lyric / 封面下载）
├── library.rs    # 命名模板 / 清洗 / 归档 / 去重 / 源文件处理
├── config.rs     # 设置 schema + 原子读写
└── commands.rs   # Tauri 命令（扫描 / 并行转换 / 进度事件）
src/              # 前端（Vite + TypeScript + 手写 CSS）
ncmdump/          # C++ 原版参考仓库（gitignore，提供 test.ncm）
```

## 致谢

- [taurusxin/ncmdump](https://github.com/taurusxin/ncmdump)（C++ 算法来源）
- [taurusxin/ncmdump-gui](https://git.taurusxin.com/taurusxin/ncmdump-gui)（界面风格参考）
- [ncmdump-go](https://git.taurusxin.com/taurusxin/ncmdump-go)（albumPic 封面直下思路）

MIT License
