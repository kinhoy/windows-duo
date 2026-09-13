# Windows Duo

当你的笔记本合上盖子（或打开盖子）时，Windows Duo 会截取当前屏幕，覆盖一个全屏透明窗口，把画面按照「绕铰链旋转」的真实透视做形变，同时按距离渐变地施加模糊和暗化 —— 模拟出真实的物理翻盖观感,像 iPhone Duo 风格折叠动画一样。

---

## 效果演示

- **合盖**：画面从清晰逐渐向后翻转、收窄，远端越来越模糊、越来越暗
- **开盖**：反向播放，从最模糊、最暗逐渐恢复清晰

画面形变、模糊强度、暗化程度都可以在设置窗口里实时调参。

---

## 下载

前往 [Releases](../../releases) 页面下载最新版 `WindowsDuo.exe`。

- 单文件，无安装包
- 需要 Windows 10 / 11 64 位
- 需要支持 DirectX 12 或 Vulkan 的显卡

---

## 使用

双击 `WindowsDuo.exe` 运行。

- **设置窗口**：启动后自动弹出，可以调所有参数
- **关闭设置窗口**：不会退出程序，而是最小化到系统托盘
- **重新打开**：双击托盘图标，或者再运行一次 exe（会自动唤出已有实例）
- **退出程序**：设置窗口里的「退出」按钮
- **合盖 / 开盖**：默认自动播放对应动画（可在设置里关闭）

演示过程中按 **Esc** 可提前结束动画。

---

### 形变模式

- **透视投影**（默认）：画面当作绕铰链旋转的平面做真透视，几何正确，观察距离 2.5~3.5 最佳
- **梯形拉伸**：旧版算法，收缩更夸张，观察距离 6 最佳

---

## 配置文件

所有参数保存在：

```
%APPDATA%\windows-duo\config.json
```

设置窗口里点「保存设置」写入，点「重置参数」恢复默认值（不会立即保存）。

---

## 从源码构建

### 环境要求

- Rust stable（1.75+）
- Windows 10 / 11 64 位
- MSVC 工具链（Visual Studio Build Tools 或完整 VS）

### 编译

```bash
git clone https://github.com/kinhoy/windows-duo.git
cd windows-duo
cargo build --release
```

### 项目结构

```
src/
├── main.rs        程序入口、单实例锁、panic 日志
├── app.rs         winit 应用主循环、三窗口管理（设置/提示/演示）
├── capture.rs     GDI 主屏截图
├── config.rs      参数结构、JSON 读写
├── depth.rs       透视几何 + 单应矩阵
├── power.rs       合盖事件监听 + 系统托盘
├── renderer.rs    wgpu 渲染管线、mip 链生成
├── shader.wgsl    主着色器（采样 + 模糊 + 暗化）
└── simulation.rs  开合角度时间线
```

---

## 已知限制

- 只抓主屏，多显示器时副屏不会有动画
- 只支持 Windows，代码里用了大量 Win32 API
- 依赖 DX12 或 Vulkan，非常老的显卡可能跑不起来
- 合盖动画只有 150ms 延迟，系统睡眠太快时可能看不到

---

## 崩溃排查

如果程序崩溃，会写入：

```
%APPDATA%\windows-duo\crash.log
```

把内容贴到 Issue 里即可。

---

## 许可

本项目采用 **Apache License 2.0** 许可。

Copyright 2026 kinhoy

---
