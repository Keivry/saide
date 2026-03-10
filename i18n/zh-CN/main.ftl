## 应用程序
app-title = SAide

## UI - 工具栏
toolbar-rotate = 旋转视频
toolbar-configure = 配置映射
toolbar-editor = 映射编辑器
toolbar-create-profile = 创建映射配置
toolbar-delete-profile = 删除映射配置
toolbar-keyboard-mapping = 切换键盘映射
toolbar-mapping-visualization = 显示映射可视化
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

## UI - 指示器浮动面板
indicator-panel-resolution = 分辨率：
indicator-panel-capture-orientation = 捕获方向：
indicator-panel-video-rotation = 视频旋转：
indicator-panel-device-rotation = 设备旋转：
indicator-panel-fps = FPS：
indicator-panel-frames = 帧数（丢失/总数）：
indicator-panel-latency-avg = 延迟（平均）：
indicator-panel-latency-p95 = 延迟（P95）：
indicator-panel-decode = 解码：
indicator-panel-gpu-upload = GPU 上传：
indicator-panel-profile = 配置：
indicator-panel-profile-none = 不可用

## UI - 映射编辑器
mapping-config-title = 映射配置模式
mapping-config-profile-label = 配置名称：
mapping-config-profile-none = 无配置
mapping-config-instruction-add = 左键 - 添加映射
mapping-config-instruction-delete = 右键 - 删除映射
mapping-config-instruction-help = F1 - 显示帮助
mapping-config-instruction-exit = 按 ESC 退出

## UI - 映射编辑器对话框
editor-dialog-create-title = 创建映射
editor-dialog-create-message =
    位置：({$x}, {$y})
    
    按任意键或 ESC 取消...
editor-dialog-delete-title = 删除映射
editor-dialog-delete-message = {$key}：({$x}, {$y})？
editor-dialog-delete-profile-title = 删除配置
editor-dialog-delete-profile-message = 确定要删除配置 "{$name}" 吗？
editor-dialog-rename-title = 重命名配置
editor-dialog-rename-placeholder = 新名称
editor-dialog-new-title = 创建新配置
editor-dialog-new-placeholder = 配置名称
editor-dialog-saveas-title = 另存为配置
editor-dialog-saveas-placeholder = 新配置名称
editor-dialog-switch-title = 选择要启用的配置
editor-dialog-error-profile-exists-title = 配置已存在
editor-dialog-error-profile-exists-message = 配置 "{$profile_name}" 已存在。
mapping-config-help-title = 帮助
mapping-config-help-message =
    可用快捷键：
    
    F1  - 显示此帮助      
    F2  - 重命名配置      
    F3  - 新建配置        
    F4  - 删除当前配置    
    F5  - 另存为          
    F6  - 切换配置        
    F7  - 切换到上一个配置
    F8  - 切换到下一个配置

    ESC - 关闭编辑器      

## UI - 通知
notification-no-active-profile = 无活动配置
notification-no-profiles = 无可用配置
notification-create-profile-failed = 创建配置失败
notification-delete-profile-failed = 删除配置失败
notification-rename-profile-failed = 重命名配置失败
notification-save-profile-as-failed = 另存配置失败
notification-switch-profile-failed = 切换配置失败
notification-add-mapping-failed = 添加映射失败
notification-delete-mapping-failed = 删除映射失败
notification-save-config-failed = 保存配置失败

## UI - 通用对话框按钮
dialog-button-confirm = 确认
dialog-button-cancel = 取消

probe-codec-dialog-title = 检测编解码器兼容性
probe-codec-dialog-message = 未找到设备 { $serial } 的编解码器配置文件。立即运行检测？（约 30-60 秒）
probe-codec-progress-title = 正在检测编解码器兼容性
probe-codec-step-detecting-device = 正在读取设备信息…
probe-codec-step-detecting-encoder = 正在检测硬件编码器…
probe-codec-step-testing-profile = 正在测试编解码器配置（{ $index }/{ $total }）：{ $name }
probe-codec-step-testing-option = 正在测试编解码器选项（{ $index }/{ $total }）：{ $key }
probe-codec-step-validating = 正在验证组合配置…
probe-codec-done-success = 检测完成
probe-codec-done-failed = 检测失败：{ $error }
