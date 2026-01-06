## 应用程序
app-title = SAide
app-starting = SAide 正在启动...
app-shutdown = 收到关闭信号，正在关闭应用程序

## 配置
config-video-backend = 视频后端: {$backend}
config-max-video-size = 最大视频尺寸: {$size}
config-max-fps = 最大 FPS: {$fps}
config-logging-level = 日志级别: {$level}

## 设备
device-serial = 设备: {$serial}
device-offline = 设备已离线 - USB/ADB 连接丢失
device-orientation-changed = 设备旋转至方向: {$orientation}
device-ime-state-changed = 设备输入法状态改变: {$state}

## 初始化
init-completed = 初始化成功完成
init-error = 初始化错误: {$error}
init-device-orientation = 初始设备方向: {$orientation}
init-video-rotation = 初始视频旋转: {$rotation}

## 连接
connection-ready = ScrcpyConnection 就绪: {$width}x{$height}, 设备: {$serial} ({$name}), 捕获方向: {$orientation}
connection-cleanup = SAideApp 正在清理连接
connection-shutdown-failed = 关闭连接失败: {$error}
connection-cleanup-completed = SAideApp 清理完成

## 视频
video-resolution = 视频分辨率: {$width}x{$height}
video-rotated = 视频旋转至 {$rotation}
video-dimensions-changed = 视频尺寸改变: {$old_width}x{$old_height} -> {$new_width}x{$new_height}

## 屏幕
screen-off-init = 根据配置关闭屏幕
screen-off-init-failed = 初始化时关闭屏幕失败: {$error}
screen-off-toolbar = 从工具栏关闭屏幕
screen-off-success = 屏幕已关闭（按物理电源键唤醒）
screen-off-failed = 关闭屏幕失败: {$error}

## 按键映射
mapping-add = 添加映射: {$key} -> ({$x}, {$y})
mapping-add-screen = 添加映射: 屏幕=({$screen_x},{$screen_y}) -> 百分比=({$percent_x},{$percent_y}) [设备方向={$orientation}]
mapping-delete = 删除映射: {$key}
mapping-delete-screen = 删除映射: {$key} 位于 ({$x}, {$y})
mapping-saved = 映射保存成功
mapping-deleted = 映射删除成功
mapping-save-failed = 保存配置失败: {$error}
mapping-keyboard-not-init = 键盘映射器未初始化
mapping-profiles-refreshed = 键盘配置刷新: 当前={$active}, 可用={$available}
mapping-profile-set = 当前配置设置为: {$profile}
mapping-profile-disabled = 为此设备/方向禁用自定义按键映射。

## 输入事件
input-skip-not-init = 跳过输入处理 - 未初始化
input-keyboard-event = 处理键盘事件: 按键={$key}, 修饰键={$modifiers}
input-keyboard-event-failed = 处理键盘事件失败: {$error}
input-text-event-failed = 处理文本输入事件失败: {$error}
input-mouse-button = 处理鼠标按键事件: {$button} 位于 {$pos}
input-mouse-button-failed = 处理鼠标按键事件失败: {$error}
input-mouse-move-failed = 处理鼠标移动事件失败: {$error}
input-mouse-release-failed = 处理鼠标释放事件失败: {$error}
input-mouse-wheel = 处理鼠标滚轮事件: {$delta} 位于 {$pos}
input-mouse-wheel-failed = 处理滚轮事件失败: {$error}
input-mouse-wheel-success = 鼠标滚轮事件位于 scrcpy 视频坐标: ({$x}, {$y})
input-mouse-mapper-update-failed = 更新鼠标映射器失败: {$error}
input-coords-convert-failed = 屏幕坐标转换为视频坐标失败
input-coords-converted = 坐标转换 屏幕 ({$screen_x}, {$screen_y}) -> scrcpy 视频 ({$scrcpy_x}, {$scrcpy_y})

## 设备监视器
monitor-skip-not-init = 跳过设备监视器处理 - 未初始化
monitor-keyboard-unavailable = 键盘映射器不可用，无法刷新配置

## UI - 工具栏
toolbar-rotate = 旋转视频
toolbar-configure = 配置映射
toolbar-keyboard-mapping = 切换键盘映射
toolbar-screen-off = 关闭屏幕
toolbar-screen-off-hint = （按物理电源键唤醒）

## UI - 音频警告
audio-warning-title = 音频不可用
audio-warning-close = ✖

## UI - 指示器
indicator-fps = FPS: {$fps}
indicator-latency = 延迟: {$ms}ms
indicator-frames = 帧数: {$total}
indicator-dropped = 丢帧: {$dropped}
indicator-profile = 配置: {$profile}
indicator-orientation = 方向: {$orientation}°
indicator-resolution = 分辨率: {$width}x{$height}

## 后台任务
background-task-cancel = SAideApp 退出，取消后台任务

## 流
stream-stop = 停止流
stream-ready = 流就绪: {$width}x{$height}
stream-resolution-changed = 分辨率改变: {$width}x{$height}
stream-failed = 流失败: {$error}
stream-worker-cancel = 流工作线程因取消而退出
stream-worker-error = 流工作线程错误: {$error}
stream-worker-send-failed = 发送 PlayerEvent::Failed 失败: {$error}

## 音频
audio-thread-start = 音频线程已启动，进入读取循环...
audio-thread-started = 音频线程已启动（Opus）
audio-thread-header = 音频线程: 尝试读取头部...
audio-thread-header-success = 音频线程: 头部读取成功
audio-packets-processed = 音频: {$count} 个数据包已处理
audio-playback-error = 音频播放错误: {$error}
audio-decode-error = 音频解码错误: {$error}
audio-thread-error = 音频线程错误: {$error}
audio-thread-cancel = 音频线程因取消而退出
audio-thread-terminated = 音频线程终止: {$error}

## 视频解码
video-decode-start = 启动视频解码循环...
video-decode-cancel = 视频解码循环因取消而退出

## Ctrl-C 处理
ctrlc-received = 收到 Ctrl-C，正在关闭...
ctrlc-handler-failed = 设置 Ctrl-C 处理程序失败: {$error}

## 平台检测
platform-nvidia-proc = 通过 /proc 检测到 NVIDIA 驱动
platform-nvidia-smi = 通过 nvidia-smi 检测到 NVIDIA GPU
platform-nvidia-drm = 通过 DRM 设备检测到 NVIDIA GPU
platform-gpu-vendor = 发现 GPU 厂商: 0x{$vendor} 位于 {$path}
platform-intel = 检测到 Intel GPU
platform-amd = 检测到 AMD GPU

## 编解码器探测
codec-probe-start = 🔍 探测设备的编解码器兼容性: {$serial}
codec-hw-encoder = 检测到硬件编码器: {$encoder}
codec-default-encoder = 使用系统默认编码器
codec-testing = 测试 {$count} 个编解码器选项...
codec-supported = ✅ 支持
codec-not-supported = ❌ 不支持
codec-validating = 🔄 验证组合配置...
codec-testing-config = 测试: {$config}
codec-combined-works = ✅ 组合配置有效！
codec-combined-failed = ❌ 组合配置失败，回退到 None
codec-final-config = 最终配置: {$config}
codec-no-options = 没有支持的选项，使用默认值
codec-test-options = 测试: video_codec_options={$options}
codec-connection-failed = 连接失败: {$error}
codec-packet-read-success = ✅ 成功读取视频数据包
codec-packet-read-failed = 读取数据包失败: {$error}
codec-profiles-saved = 设备配置已保存至 {$path}
codec-skip-latency = 跳过 'latency'（需要 Android 11+）
codec-skip-bframes = 跳过 'max-bframes'（需要 Android 13+）
