//! 激活状态机：把全局热键的原始事件翻译成「开始/停止录音」的指令。
//!
//! 这是产品核心交互（按住说话 / 单击切换）的纯逻辑实现：
//! 不依赖 Tauri、不碰麦克风，因此可以被确定性地单元测试。
//! 托盘和全局热键只是它的外壳——把事件喂进来、按返回的 Effect 行动。
//!
//! 现支持**两个独立热键同时生效**：长按键（Hold，按住说话松手停）和
//! 免按键（Toggle，按一下开始、再按一下停）。录音中只认开启它的那个源，
//! 另一个源的事件一律忽略，避免互相打架。

/// 哪个热键触发的事件（长按键 / 免按键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// 长按键：按下开始录音，松开结束。
    Hold,
    /// 免按键：按一下开始，再按一下结束。
    Toggle,
}

/// 录音状态（供 UI 状态指示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
}

/// 来自全局热键的原始事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

/// 状态机要求宿主执行的副作用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    StartRecording,
    StopRecording,
    /// 取消并丢弃本次录音（不出稿、不注入）。
    CancelRecording,
    None,
}

/// 激活状态机本体。`recording` 记录当前是否在录音以及由哪个源开启。
#[derive(Debug, Clone, Default)]
pub struct Activation {
    recording: Option<Trigger>,
}

impl Activation {
    pub fn new() -> Self {
        Self { recording: None }
    }

    pub fn state(&self) -> RecordingState {
        if self.recording.is_some() {
            RecordingState::Recording
        } else {
            RecordingState::Idle
        }
    }

    /// 处理某个热键源的一个事件，推进状态并返回需要执行的副作用。
    pub fn handle(&mut self, trigger: Trigger, event: HotkeyEvent) -> Effect {
        use HotkeyEvent::*;
        use Trigger::*;

        match (self.recording, trigger, event) {
            // 空闲时：任一热键「按下」都开始录音，并记住是哪个源开的。
            (None, t, Pressed) => {
                self.recording = Some(t);
                Effect::StartRecording
            }
            // 长按键开的录音：同一个长按键「松开」→ 停止。
            (Some(Hold), Hold, Released) => {
                self.recording = None;
                Effect::StopRecording
            }
            // 免按键开的录音：同一个免按键再次「按下」→ 停止。
            (Some(Toggle), Toggle, Pressed) => {
                self.recording = None;
                Effect::StopRecording
            }
            // 其余一律无副作用，关键的有：
            // - 长按时 OS 持续重复发来的 Pressed（必须幂等）
            // - 免按键开的录音收到 Released
            // - 录音中来自「另一个源」的任何事件（交叉忽略，不互相打断）
            // - 空闲时的 Released
            _ => Effect::None,
        }
    }

    /// 回车「确认」：仅当当前录音由免按键（双击）开启时才停止并出稿。
    /// 长按录音中或空闲时一律不动——尤其空闲时绝不会因回车误启动录音。
    pub fn confirm(&mut self) -> Effect {
        if self.recording == Some(Trigger::Toggle) {
            self.recording = None;
            Effect::StopRecording
        } else {
            Effect::None
        }
    }

    /// Esc「取消」：仅当当前录音由免按键（双击）开启时才中止并丢弃（不出稿）。
    /// 长按录音中或空闲时一律不动。
    pub fn cancel(&mut self) -> Effect {
        if self.recording == Some(Trigger::Toggle) {
            self.recording = None;
            Effect::CancelRecording
        } else {
            Effect::None
        }
    }

    /// 当前是否处于「免按键（双击）开启」的录音。供键盘层判断录音中是否拦截回车 / Esc。
    pub fn is_toggle_recording(&self) -> bool {
        self.recording == Some(Trigger::Toggle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_press_starts_and_release_stops() {
        let mut a = Activation::new();
        assert_eq!(a.state(), RecordingState::Idle);

        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Pressed), Effect::StartRecording);
        assert_eq!(a.state(), RecordingState::Recording);

        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Released), Effect::StopRecording);
        assert_eq!(a.state(), RecordingState::Idle);
    }

    #[test]
    fn hold_ignores_os_key_repeat() {
        let mut a = Activation::new();
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Pressed), Effect::StartRecording);
        // 长按时 OS 会持续发 Pressed —— 不能重复触发「开始」。
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Pressed), Effect::None);
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Pressed), Effect::None);
        assert_eq!(a.state(), RecordingState::Recording);
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Released), Effect::StopRecording);
    }

    #[test]
    fn hold_release_while_idle_is_noop() {
        let mut a = Activation::new();
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Released), Effect::None);
        assert_eq!(a.state(), RecordingState::Idle);
    }

    #[test]
    fn toggle_press_flips_start_then_stop() {
        let mut a = Activation::new();
        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Pressed), Effect::StartRecording);
        assert_eq!(a.state(), RecordingState::Recording);

        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Pressed), Effect::StopRecording);
        assert_eq!(a.state(), RecordingState::Idle);
    }

    #[test]
    fn toggle_ignores_releases() {
        let mut a = Activation::new();
        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Released), Effect::None);

        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Pressed), Effect::StartRecording);
        // 免按模式下松开不应停止录音。
        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Released), Effect::None);
        assert_eq!(a.state(), RecordingState::Recording);
    }

    #[test]
    fn other_source_is_ignored_while_recording() {
        // 长按键开的录音，期间按免按键不应打断。
        let mut a = Activation::new();
        a.handle(Trigger::Hold, HotkeyEvent::Pressed);
        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Pressed), Effect::None);
        assert_eq!(a.state(), RecordingState::Recording);
        // 仍由长按键松手停止。
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Released), Effect::StopRecording);

        // 反过来：免按键开的录音，期间按/松长按键都不影响。
        a.handle(Trigger::Toggle, HotkeyEvent::Pressed);
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Pressed), Effect::None);
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Released), Effect::None);
        assert_eq!(a.state(), RecordingState::Recording);
        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Pressed), Effect::StopRecording);
    }

    #[test]
    fn either_key_can_start_when_idle() {
        let mut a = Activation::new();
        assert_eq!(a.handle(Trigger::Toggle, HotkeyEvent::Pressed), Effect::StartRecording);
        a.handle(Trigger::Toggle, HotkeyEvent::Pressed); // 停
        assert_eq!(a.state(), RecordingState::Idle);
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Pressed), Effect::StartRecording);
    }

    #[test]
    fn confirm_stops_only_toggle_recording() {
        let mut a = Activation::new();

        // 空闲时确认是 no-op，绝不会误启动录音。
        assert_eq!(a.confirm(), Effect::None);
        assert_eq!(a.state(), RecordingState::Idle);
        assert!(!a.is_toggle_recording());

        // 免按键录音中：确认 → 停止出稿。
        a.handle(Trigger::Toggle, HotkeyEvent::Pressed);
        assert!(a.is_toggle_recording());
        assert_eq!(a.confirm(), Effect::StopRecording);
        assert_eq!(a.state(), RecordingState::Idle);
        assert!(!a.is_toggle_recording());

        // 长按录音中：确认不打断（按住说话不应被回车停掉），仍由松手结束。
        a.handle(Trigger::Hold, HotkeyEvent::Pressed);
        assert!(!a.is_toggle_recording());
        assert_eq!(a.confirm(), Effect::None);
        assert_eq!(a.state(), RecordingState::Recording);
        assert_eq!(a.handle(Trigger::Hold, HotkeyEvent::Released), Effect::StopRecording);
    }

    #[test]
    fn cancel_aborts_only_toggle_recording() {
        let mut a = Activation::new();

        // 空闲时取消是 no-op，绝不会误启动录音。
        assert_eq!(a.cancel(), Effect::None);
        assert_eq!(a.state(), RecordingState::Idle);

        // 免按键录音中：取消 → 中止丢弃。
        a.handle(Trigger::Toggle, HotkeyEvent::Pressed);
        assert_eq!(a.cancel(), Effect::CancelRecording);
        assert_eq!(a.state(), RecordingState::Idle);

        // 长按录音中：取消不打断（仍由松手结束）。
        a.handle(Trigger::Hold, HotkeyEvent::Pressed);
        assert_eq!(a.cancel(), Effect::None);
        assert_eq!(a.state(), RecordingState::Recording);
    }
}
