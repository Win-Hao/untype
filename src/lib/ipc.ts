import { invoke } from "@tauri-apps/api/core";

// ============================================================
// 后端 IPC 类型化封装 —— 对应 src-tauri 的 26 个 #[tauri::command]。
// 契约与原 Svelte 版完全一致，后端不动。
// ============================================================

export type LlmSettings = {
  base_url: string;
  api_key: string;
  model: string;
  disable_thinking: boolean;
  keys: Record<string, string>;
};

export type AsrConfig = {
  engine: string; // "local" | "cloud"
  cloud_vendor: string; // "volc" | "ali"
  volc_api_key: string;
  volc_resource_id: string;
  ali_api_key: string;
  ali_model: string;
};

export type Replacement = { wrong: string; right: string };

export type VocabData = {
  soft_terms: string[];
  pinyin_terms: string[];
  replacements: Replacement[];
};

export type LearnedPair = { wrong: string; right: string; count: number };

export type Which = "hold" | "toggle";

// ---- 录音 / 引导 ----
export const getRecording = () => invoke<boolean>("get_recording");
export const getOnboarded = () => invoke<boolean>("get_onboarded");
export const completeOnboarding = () => invoke<void>("complete_onboarding");

// ---- 快捷键 ----
export const getShortcuts = () => invoke<[string, string]>("get_shortcuts");
export const setShortcut = (which: Which, accelerator: string) =>
  invoke<void>("set_shortcut", { which, accelerator });

// ---- 整理风格 ----
export const getPolishStyle = () => invoke<string>("get_polish_style");
export const setPolishStyle = (style: string) =>
  invoke<void>("set_polish_style", { style });

// ---- 麦克风 ----
export const listMicrophones = () => invoke<string[]>("list_microphones");
export const getMicrophone = () => invoke<string>("get_microphone");
export const setMicrophone = (name: string) =>
  invoke<void>("set_microphone", { name });
export const startMicMonitor = (device: string) =>
  invoke<void>("start_mic_monitor", { device });
export const stopMicMonitor = () => invoke<void>("stop_mic_monitor");

// ---- LLM 设置 ----
export const getLlmSettings = () => invoke<LlmSettings>("get_llm_settings");
export const setLlmSettings = (cfg: LlmSettings) =>
  invoke<void>("set_llm_settings", { cfg });

// ---- ASR 引擎 ----
export const getAsrConfig = () => invoke<AsrConfig>("get_asr_config");
export const setAsrConfig = (cfg: AsrConfig) =>
  invoke<void>("set_asr_config", { cfg });

// ---- 词典 ----
export const getVocab = () => invoke<VocabData>("get_vocab");
export const setVocab = (data: VocabData) => invoke<void>("set_vocab", { data });

// ---- 自学建议 ----
export const getLearnedSuggestions = () =>
  invoke<LearnedPair[]>("get_learned_suggestions");
export const acceptLearnedSuggestion = (right: string) =>
  invoke<void>("accept_learned_suggestion", { right });
export const dismissLearnedSuggestion = (wrong: string, right: string) =>
  invoke<void>("dismiss_learned_suggestion", { wrong, right });

// ---- 辅助功能 / 系统 ----
export const checkAccessibility = () => invoke<boolean>("check_accessibility");
export const requestAccessibility = () => invoke<void>("request_accessibility");
export const resetAccessibilityTcc = () =>
  invoke<void>("reset_accessibility_tcc");
export const restartApp = () => invoke<void>("restart_app");
// 注：应用内更新走 @tauri-apps/plugin-updater（见 src/lib/useUpdater.ts），不再走自写命令。
