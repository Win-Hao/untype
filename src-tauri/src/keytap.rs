//! 全局热键：macOS 用 CGEventTap 监听键盘，支持「单个键」（含单修饰键、区分左右）
//! 以及组合键。长按键 = 按住说话松手停；免按键 = 双击触发。
//! 需要「辅助功能」权限（与文字注入共用）。非 macOS 暂为占位（Windows 待 Phase 4）。
//!
//! 键事件匹配（`Matcher` / `parse_spec`）是不依赖系统的纯逻辑，可确定性单测；
//! CGEventTap 那层只负责把 CGEvent 翻译成 (keycode, 事件种类) 喂进来。

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::activation::{HotkeyEvent, Trigger};

/// 免按模式双击的最大间隔。
const DOUBLE_CLICK: Duration = Duration::from_millis(350);

/// 录音中可被拦截的「控制键」的 macOS 虚拟 keycode。
const ENTER_RETURN: i64 = 36; // 主键盘回车
const ENTER_KEYPAD: i64 = 76; // 小键盘回车
const ESCAPE: i64 = 53; // Esc

/// 键盘层产出的动作：命中某个热键，或免按键录音中的控制键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// 命中某个长按 / 免按热键。
    Hotkey(Trigger, HotkeyEvent),
    /// 免按键录音中按回车 —— 确认本次听写（停止录音并出稿）。
    Confirm,
    /// 免按键录音中按 Esc —— 取消本次听写（中止录音、丢弃，不出稿）。
    Cancel,
}

/// 录音中可拦截的控制键 → 对应动作；其余键返回 None（不拦截）。
fn control_action(keycode: i64) -> Option<KeyAction> {
    match keycode {
        ENTER_RETURN | ENTER_KEYPAD => Some(KeyAction::Confirm),
        ESCAPE => Some(KeyAction::Cancel),
        _ => None,
    }
}

/// 控制键事件在键盘层的处置结果（纯逻辑，可单测）。
/// 回车确认与 Esc 取消同构，仅 `emit` 出的动作不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlOutcome {
    /// 是否吞掉该事件（true = 不传给前台 app）。
    pub drop: bool,
    /// 是否发出该控制键对应的动作（Confirm / Cancel）。
    pub emit: bool,
    /// 处理后这个键是否处于「吞咽窗口」中
    /// （用于配对吞掉 autorepeat 重复按下与最终松开）。
    pub swallow: bool,
}

/// 决定一个控制键事件如何处置。
///
/// 仅当「免按键录音中」首次按下控制键，才拦截并发出动作；一旦进入吞咽窗口，
/// 这次按压期间的 autorepeat 重复按下与最终松开都一并吞掉，避免前台 app
/// 收到半个按键。其余情况（未录音、长按录音）一律放行。
/// `swallowing_this` = 当前是否正吞着「这个」控制键。
pub(crate) fn control_outcome(
    kind: EventKind,
    toggle_recording: bool,
    swallowing_this: bool,
) -> ControlOutcome {
    match kind {
        EventKind::KeyDown => {
            if swallowing_this {
                // 吞咽窗口内的重复按下（autorepeat）：继续吞，但不重复发动作。
                ControlOutcome { drop: true, emit: false, swallow: true }
            } else if toggle_recording {
                // 免按键录音中首次按下：发一次动作，并开始吞这次按压。
                ControlOutcome { drop: true, emit: true, swallow: true }
            } else {
                // 未录音 / 长按录音：照常给前台。
                ControlOutcome { drop: false, emit: false, swallow: false }
            }
        }
        EventKind::KeyUp => {
            if swallowing_this {
                // 吞掉与被拦截按下配对的松开（此时录音多半已停、toggle_recording 已 false，
                // 仍要靠吞咽窗口吞掉，避免前台收到孤立的松开）。
                ControlOutcome { drop: true, emit: false, swallow: false }
            } else {
                ControlOutcome { drop: false, emit: false, swallow: false }
            }
        }
        // 控制键不会以 FlagsChanged 形式出现；保持原状放行。
        EventKind::FlagsChanged => ControlOutcome { drop: false, emit: false, swallow: swallowing_this },
    }
}

/// 组合键需要的修饰类别（不区分左右）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModSet {
    cmd: bool,
    alt: bool,
    ctrl: bool,
    shift: bool,
}

/// 一个键绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    /// 单个键（修饰键区分左右 / 功能键）：直接比 keycode。
    Single(i64),
    /// 组合键：主键 keycode + 需要的修饰类别。
    Combo { key: i64, mods: ModSet },
}

/// 两个模式当前的绑定（None = 未绑定 / 停用）。
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    pub hold: Option<Spec>,
    pub toggle: Option<Spec>,
}

impl Bindings {
    pub fn from_strs(hold: &str, toggle: &str) -> Self {
        Self {
            hold: parse_spec(hold),
            toggle: parse_spec(toggle),
        }
    }
}

/// e.code 风格 token → macOS 虚拟 keycode。
fn token_to_keycode(t: &str) -> Option<i64> {
    Some(match t {
        // 修饰键（区分左右）
        "MetaLeft" => 55, "MetaRight" => 54,
        "ShiftLeft" => 56, "ShiftRight" => 60,
        "AltLeft" => 58, "AltRight" => 61,
        "ControlLeft" => 59, "ControlRight" => 62,
        // 功能键
        "F1" => 122, "F2" => 120, "F3" => 99, "F4" => 118, "F5" => 96, "F6" => 97,
        "F7" => 98, "F8" => 100, "F9" => 101, "F10" => 109, "F11" => 103, "F12" => 111,
        "F13" => 105, "F14" => 107, "F15" => 113, "F16" => 106, "F17" => 64, "F18" => 79,
        "F19" => 80, "F20" => 90,
        // 字母
        "KeyA" => 0, "KeyB" => 11, "KeyC" => 8, "KeyD" => 2, "KeyE" => 14, "KeyF" => 3,
        "KeyG" => 5, "KeyH" => 4, "KeyI" => 34, "KeyJ" => 38, "KeyK" => 40, "KeyL" => 37,
        "KeyM" => 46, "KeyN" => 45, "KeyO" => 31, "KeyP" => 35, "KeyQ" => 12, "KeyR" => 15,
        "KeyS" => 1, "KeyT" => 17, "KeyU" => 32, "KeyV" => 9, "KeyW" => 13, "KeyX" => 7,
        "KeyY" => 16, "KeyZ" => 6,
        // 数字
        "Digit0" => 29, "Digit1" => 18, "Digit2" => 19, "Digit3" => 20, "Digit4" => 21,
        "Digit5" => 23, "Digit6" => 22, "Digit7" => 26, "Digit8" => 28, "Digit9" => 25,
        "Space" => 49,
        _ => return None,
    })
}

/// 组合键修饰 token → 类别标记。
fn mod_token(t: &str) -> Option<&'static str> {
    Some(match t {
        "Meta" | "Command" | "Cmd" | "Super" => "cmd",
        "Alt" | "Option" => "alt",
        "Control" | "Ctrl" => "ctrl",
        "Shift" => "shift",
        _ => return None,
    })
}

/// 解析键绑定字符串（空 / 非法 → None）。
pub fn parse_spec(s: &str) -> Option<Spec> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split('+').collect();
    if parts.len() == 1 {
        return token_to_keycode(parts[0]).map(Spec::Single);
    }
    let (key_tok, mod_toks) = parts.split_last().unwrap();
    let key = token_to_keycode(key_tok)?;
    let mut mods = ModSet::default();
    for m in mod_toks {
        match mod_token(m)? {
            "cmd" => mods.cmd = true,
            "alt" => mods.alt = true,
            "ctrl" => mods.ctrl = true,
            "shift" => mods.shift = true,
            _ => return None,
        }
    }
    Some(Spec::Combo { key, mods })
}

fn has_cmd(p: &HashSet<i64>) -> bool { p.contains(&55) || p.contains(&54) }
fn has_alt(p: &HashSet<i64>) -> bool { p.contains(&58) || p.contains(&61) }
fn has_ctrl(p: &HashSet<i64>) -> bool { p.contains(&59) || p.contains(&62) }
fn has_shift(p: &HashSet<i64>) -> bool { p.contains(&56) || p.contains(&60) }

/// 组合键修饰精确匹配（所需的都在、不需要的都不在）。
fn mods_match(m: &ModSet, p: &HashSet<i64>) -> bool {
    has_cmd(p) == m.cmd && has_alt(p) == m.alt && has_ctrl(p) == m.ctrl && has_shift(p) == m.shift
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    KeyDown,
    KeyUp,
    FlagsChanged,
}

/// 键事件匹配器（纯逻辑）。维护当前按下集合 + 免按双击计时。
pub struct Matcher {
    pressed: HashSet<i64>,
    last_toggle_press: Option<Instant>,
}

impl Default for Matcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Matcher {
    pub fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            last_toggle_press: None,
        }
    }

    /// 喂入一个键事件，更新内部状态，返回需要触发的 (Trigger, HotkeyEvent)。
    pub fn on_event(
        &mut self,
        kind: EventKind,
        keycode: i64,
        now: Instant,
        hold: Option<&Spec>,
        toggle: Option<&Spec>,
    ) -> Option<(Trigger, HotkeyEvent)> {
        // 1) 更新按下集合，得出该键此刻是「按下」还是「松开」。
        //    修饰键没有独立的 up/down 事件，靠 FlagsChanged 对同一 keycode 交替判定。
        let press = match kind {
            EventKind::KeyDown => {
                self.pressed.insert(keycode);
                true
            }
            EventKind::KeyUp => {
                self.pressed.remove(&keycode);
                false
            }
            EventKind::FlagsChanged => {
                if self.pressed.remove(&keycode) {
                    false
                } else {
                    self.pressed.insert(keycode);
                    true
                }
            }
        };

        // 2) 长按键：直接映射成 Pressed / Released。
        if let Some(spec) = hold {
            if let Some(ev) = match_hold(spec, keycode, press, &self.pressed) {
                return Some((Trigger::Hold, ev));
            }
        }

        // 3) 免按键：在「按下沿」且匹配时做双击检测，双击才发一次 Toggle Pressed。
        if let Some(spec) = toggle {
            if press && spec_press_matches(spec, keycode, &self.pressed) {
                if let Some(t) = self.last_toggle_press {
                    if now.duration_since(t) <= DOUBLE_CLICK {
                        self.last_toggle_press = None;
                        return Some((Trigger::Toggle, HotkeyEvent::Pressed));
                    }
                }
                self.last_toggle_press = Some(now);
            }
        }
        None
    }
}

fn match_hold(spec: &Spec, keycode: i64, press: bool, pressed: &HashSet<i64>) -> Option<HotkeyEvent> {
    match spec {
        Spec::Single(code) => {
            if keycode == *code {
                Some(if press {
                    HotkeyEvent::Pressed
                } else {
                    HotkeyEvent::Released
                })
            } else {
                None
            }
        }
        Spec::Combo { key, mods } => {
            if keycode != *key {
                return None;
            }
            if press {
                mods_match(mods, pressed).then_some(HotkeyEvent::Pressed)
            } else {
                // 主键松开即结束（activation 对未开始的 Released 是幂等的）
                Some(HotkeyEvent::Released)
            }
        }
    }
}

fn spec_press_matches(spec: &Spec, keycode: i64, pressed: &HashSet<i64>) -> bool {
    match spec {
        Spec::Single(code) => keycode == *code,
        Spec::Combo { key, mods } => keycode == *key && mods_match(mods, pressed),
    }
}

/// 在独立线程启动 CGEventTap，把命中的热键事件经 channel 发出。
/// 改键无需重启 tap：回调每次读 `bindings`，主线程改了即时生效。
///
/// tap 用 `Default`（而非 ListenOnly）以便在免按键录音中**拦截回车**作「确认」：
/// 这类回车不会传给前台 app（否则会先发出空消息/换行，识别文字却要稍后才注入，顺序错乱）。
/// `toggle_recording` 由主线程在录音状态变化时更新，回调据此判断是否拦截。
/// 回调只做读锁 + 纯逻辑 + 非阻塞 send，足够快，不会触发 tap 超时。
#[cfg(target_os = "macos")]
pub fn spawn(
    bindings: Arc<Mutex<Bindings>>,
    toggle_recording: Arc<AtomicBool>,
    tx: Sender<KeyAction>,
) {
    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::Ordering;

    std::thread::spawn(move || {
        let matcher = RefCell::new(Matcher::new());
        // 当前正吞着的控制键 keycode（None = 没在吞）；仅本回调线程访问。
        let swallowing: Cell<Option<i64>> = Cell::new(None);
        let result = CGEventTap::with_enabled(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            // Default（可改写/丢弃）而非 ListenOnly，否则下面拦截回车的 Drop 会被系统忽略。
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            |_proxy, etype, event| {
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);

                // 控制键（回车确认 / Esc 取消）：免按键录音中拦截，不放给前台 app。
                // 放在 autorepeat 过滤之前——按住期间的重复按下也要一并吞掉。
                if let Some(action) = control_action(keycode) {
                    let kind = match etype {
                        CGEventType::KeyDown => EventKind::KeyDown,
                        CGEventType::KeyUp => EventKind::KeyUp,
                        _ => return CallbackResult::Keep,
                    };
                    let swallowing_this = swallowing.get() == Some(keycode);
                    let o = control_outcome(
                        kind,
                        toggle_recording.load(Ordering::SeqCst),
                        swallowing_this,
                    );
                    if o.swallow {
                        swallowing.set(Some(keycode));
                    } else if swallowing_this {
                        swallowing.set(None);
                    }
                    if o.emit {
                        let _ = tx.send(action);
                    }
                    return if o.drop {
                        CallbackResult::Drop
                    } else {
                        CallbackResult::Keep
                    };
                }

                // 其余键：只监听不拦截，照常放行（CallbackResult::Keep）。
                let kind = match etype {
                    CGEventType::KeyDown => {
                        // 忽略系统长按重复，否则长按会被当成多次按下
                        if event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0
                        {
                            return CallbackResult::Keep;
                        }
                        EventKind::KeyDown
                    }
                    CGEventType::KeyUp => EventKind::KeyUp,
                    CGEventType::FlagsChanged => EventKind::FlagsChanged,
                    _ => return CallbackResult::Keep,
                };
                let (hold, toggle) = {
                    let b = bindings.lock().unwrap();
                    (b.hold.clone(), b.toggle.clone())
                };
                let out = matcher.borrow_mut().on_event(
                    kind,
                    keycode,
                    Instant::now(),
                    hold.as_ref(),
                    toggle.as_ref(),
                );
                if let Some((trigger, ev)) = out {
                    let _ = tx.send(KeyAction::Hotkey(trigger, ev));
                }
                CallbackResult::Keep
            },
            CFRunLoop::run_current,
        );
        if result.is_err() {
            eprintln!(
                "CGEventTap 启动失败：多半未授予「辅助功能」权限，单键热键暂不可用（授权后重启 app 生效）"
            );
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(
    _bindings: Arc<Mutex<Bindings>>,
    _toggle_recording: Arc<AtomicBool>,
    _tx: Sender<KeyAction>,
) {
    // TODO(Phase 4): Windows 用低级键盘钩子（SetWindowsHookEx）实现单键 / 区分左右。
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_modifier_and_combo() {
        assert_eq!(parse_spec("AltRight"), Some(Spec::Single(61)));
        assert_eq!(parse_spec("F5"), Some(Spec::Single(96)));
        assert_eq!(
            parse_spec("Alt+Space"),
            Some(Spec::Combo {
                key: 49,
                mods: ModSet { alt: true, ..Default::default() }
            })
        );
        assert_eq!(
            parse_spec("Control+Shift+KeyK"),
            Some(Spec::Combo {
                key: 40,
                mods: ModSet { ctrl: true, shift: true, ..Default::default() }
            })
        );
        assert_eq!(parse_spec(""), None);
        assert_eq!(parse_spec("Bogus"), None);
    }

    #[test]
    fn hold_single_modifier_press_then_release() {
        let mut m = Matcher::new();
        let hold = parse_spec("AltRight"); // keycode 61
        let t = Instant::now();
        assert_eq!(
            m.on_event(EventKind::FlagsChanged, 61, t, hold.as_ref(), None),
            Some((Trigger::Hold, HotkeyEvent::Pressed))
        );
        assert_eq!(
            m.on_event(EventKind::FlagsChanged, 61, t, hold.as_ref(), None),
            Some((Trigger::Hold, HotkeyEvent::Released))
        );
    }

    #[test]
    fn hold_distinguishes_left_from_right() {
        let mut m = Matcher::new();
        let hold = parse_spec("AltRight"); // 61
        let t = Instant::now();
        assert_eq!(m.on_event(EventKind::FlagsChanged, 58, t, hold.as_ref(), None), None); // 左 Option
        assert_eq!(
            m.on_event(EventKind::FlagsChanged, 61, t, hold.as_ref(), None),
            Some((Trigger::Hold, HotkeyEvent::Pressed))
        );
    }

    #[test]
    fn toggle_fires_only_on_double_press_within_window() {
        let mut m = Matcher::new();
        let toggle = parse_spec("ControlLeft"); // 59
        let t0 = Instant::now();
        assert_eq!(m.on_event(EventKind::FlagsChanged, 59, t0, None, toggle.as_ref()), None);
        assert_eq!(m.on_event(EventKind::FlagsChanged, 59, t0, None, toggle.as_ref()), None); // 松开
        let t1 = t0 + Duration::from_millis(200);
        assert_eq!(
            m.on_event(EventKind::FlagsChanged, 59, t1, None, toggle.as_ref()),
            Some((Trigger::Toggle, HotkeyEvent::Pressed))
        );
    }

    #[test]
    fn toggle_too_slow_is_not_double_click() {
        let mut m = Matcher::new();
        let toggle = parse_spec("ControlLeft");
        let t0 = Instant::now();
        m.on_event(EventKind::FlagsChanged, 59, t0, None, toggle.as_ref());
        m.on_event(EventKind::FlagsChanged, 59, t0, None, toggle.as_ref());
        let t1 = t0 + Duration::from_millis(600);
        assert_eq!(m.on_event(EventKind::FlagsChanged, 59, t1, None, toggle.as_ref()), None);
    }

    #[test]
    fn combo_hold_requires_modifier_then_releases_on_key_up() {
        let mut m = Matcher::new();
        let hold = parse_spec("Alt+Space"); // key 49 + alt
        let t = Instant::now();
        // 没按修饰键直接按 Space → 不触发
        assert_eq!(m.on_event(EventKind::KeyDown, 49, t, hold.as_ref(), None), None);
        m.on_event(EventKind::KeyUp, 49, t, hold.as_ref(), None);
        // 先按左 Alt(58)，再按 Space → 触发
        m.on_event(EventKind::FlagsChanged, 58, t, hold.as_ref(), None);
        assert_eq!(
            m.on_event(EventKind::KeyDown, 49, t, hold.as_ref(), None),
            Some((Trigger::Hold, HotkeyEvent::Pressed))
        );
        assert_eq!(
            m.on_event(EventKind::KeyUp, 49, t, hold.as_ref(), None),
            Some((Trigger::Hold, HotkeyEvent::Released))
        );
    }

    #[test]
    fn other_keys_do_not_trigger() {
        let mut m = Matcher::new();
        let hold = parse_spec("AltRight");
        let toggle = parse_spec("ControlLeft");
        let t = Instant::now();
        // 无关键（KeyD=2）不触发任何东西
        assert_eq!(m.on_event(EventKind::KeyDown, 2, t, hold.as_ref(), toggle.as_ref()), None);
        assert_eq!(m.on_event(EventKind::KeyUp, 2, t, hold.as_ref(), toggle.as_ref()), None);
    }

    #[test]
    fn control_action_maps_enter_and_escape() {
        assert_eq!(control_action(ENTER_RETURN), Some(KeyAction::Confirm));
        assert_eq!(control_action(ENTER_KEYPAD), Some(KeyAction::Confirm));
        assert_eq!(control_action(ESCAPE), Some(KeyAction::Cancel));
        assert_eq!(control_action(0), None); // KeyA 不是控制键
    }

    #[test]
    fn control_key_ignored_when_not_toggle_recording() {
        // 未处于免按键录音：控制键一律放行，不吞不发动作。
        assert_eq!(
            control_outcome(EventKind::KeyDown, false, false),
            ControlOutcome { drop: false, emit: false, swallow: false }
        );
        assert_eq!(
            control_outcome(EventKind::KeyUp, false, false),
            ControlOutcome { drop: false, emit: false, swallow: false }
        );
    }

    #[test]
    fn control_key_emits_and_swallows_during_toggle_recording() {
        // 首次按下 → 发动作 + 吞 + 进入吞咽窗口。
        assert_eq!(
            control_outcome(EventKind::KeyDown, true, false),
            ControlOutcome { drop: true, emit: true, swallow: true }
        );
        // 吞咽窗口内的 autorepeat 重复按下：继续吞，但不再发动作。
        assert_eq!(
            control_outcome(EventKind::KeyDown, true, true),
            ControlOutcome { drop: true, emit: false, swallow: true }
        );
        // 松开：吞掉配对的松开并退出吞咽窗口。
        assert_eq!(
            control_outcome(EventKind::KeyUp, true, true),
            ControlOutcome { drop: true, emit: false, swallow: false }
        );
    }

    #[test]
    fn control_key_release_swallowed_after_recording_stopped() {
        // 发出动作后录音随即停止（toggle_recording 已变 false），但配对的松开
        // 仍要靠吞咽窗口吞掉，避免前台收到孤立的松开。
        assert_eq!(
            control_outcome(EventKind::KeyUp, false, true),
            ControlOutcome { drop: true, emit: false, swallow: false }
        );
    }
}
