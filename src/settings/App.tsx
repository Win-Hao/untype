import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AnimatePresence, motion } from "framer-motion";
import {
  Mic,
  Sparkles,
  Info,
  Check,
  ExternalLink,
  TriangleAlert,
  ArrowRight,
  X,
  BookText,
  Plus,
  AudioLines,
  ChevronDown,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useTauriEvent } from "@/lib/useTauriEvent";
import { MOD_KEYS, mainKeyToken, prettyShortcut } from "@/lib/hotkey";
import * as ipc from "@/lib/ipc";
import type {
  LearnedPair,
  LlmSettings,
  Replacement,
  VocabData,
} from "@/lib/ipc";
import { UpdateModal } from "./UpdateModal";
import { useUpdater, type UpdateState } from "@/lib/useUpdater";

// 录音胶囊 / 选麦频谱：必须与 src-tauri/src/viz.rs 的 N_BANDS 一致
const N_BANDS = 11;
const WAVE_BARS = N_BANDS * 2 - 1; // 21：镜像后条数

type Section = "voice" | "engine" | "ai" | "dict" | "about";

const NAV = [
  { id: "voice", label: "语音输入", icon: Mic },
  { id: "engine", label: "识别引擎", icon: AudioLines },
  { id: "ai", label: "AI 整理", icon: Sparkles },
  { id: "dict", label: "词典", icon: BookText },
  { id: "about", label: "关于", icon: Info },
] as const;

const SECTION_TITLE: Record<Section, string> = {
  voice: "语音输入",
  engine: "识别引擎",
  ai: "AI 整理",
  dict: "词典",
  about: "关于",
};

// 两个独立热键行：长按 / 免按
const KEY_ROWS = [
  { which: "hold" as const, title: "长按模式", desc: "按住说话，松手结束" },
  { which: "toggle" as const, title: "免按模式", desc: "双击开始说话，再次双击结束" },
];

// 润色风格预设
const STYLES = [
  { id: "default", title: "智能整理", desc: "去口水词、纠错、按需分点（推荐）" },
  { id: "bullets", title: "列要点", desc: "整理成简洁的要点列表" },
  { id: "email", title: "邮件", desc: "整理成礼貌得体的邮件正文" },
  { id: "raw", title: "纯逐字稿", desc: "只转写、不润色，最快最忠实" },
];

// ---- BYOK 供应商预设（OpenAI 兼容；新增厂商只需加一条）----
type Provider = {
  id: string;
  name: string;
  note: string;
  free: boolean;
  baseUrl: string;
  models: string[];
  register: string;
  disableThinking: boolean;
};
const PROVIDERS: Provider[] = [
  { id: "deepseek", name: "DeepSeek", note: "极便宜、中文强（推荐）", free: false, baseUrl: "https://api.deepseek.com/v1", models: ["deepseek-chat"], register: "https://platform.deepseek.com", disableThinking: true },
  { id: "zhipu", name: "智谱 GLM", note: "中文强、有免费档", free: true, baseUrl: "https://open.bigmodel.cn/api/paas/v4", models: ["glm-4-flash", "glm-4.6"], register: "https://open.bigmodel.cn", disableThinking: true },
  { id: "qwen", name: "通义 Qwen（百炼）", note: "中文强、与 ASR 同源、有免费档", free: true, baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", models: ["qwen-plus"], register: "https://bailian.console.aliyun.com", disableThinking: false },
  { id: "moonshot", name: "Moonshot Kimi", note: "中文、长上下文", free: false, baseUrl: "https://api.moonshot.cn/v1", models: ["moonshot-v1-8k"], register: "https://platform.moonshot.cn", disableThinking: true },
  { id: "gemini", name: "Google Gemini", note: "Flash 免费档", free: true, baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", models: ["gemini-2.0-flash"], register: "https://aistudio.google.com", disableThinking: false },
  { id: "openai", name: "OpenAI", note: "gpt-4o-mini 等", free: false, baseUrl: "https://api.openai.com/v1", models: ["gpt-4o-mini"], register: "https://platform.openai.com", disableThinking: false },
  { id: "custom", name: "自定义（任意 OpenAI 兼容）", note: "手动填 base_url 与模型", free: false, baseUrl: "", models: [], register: "", disableThinking: false },
];

const ALI_BASE = "https://dashscope.aliyuncs.com/compatible-mode/v1";

// 更新提醒克制策略（localStorage）：
//  - 「跳过此版本」记被跳过的版本号，该版本不再自动弹;
//  - 「稍后」记时间戳，冷却期内启动不自动弹（仍可关于页手动检查）。
const SKIP_KEY = "untype.skipUpdateVersion";
const POSTPONE_KEY = "untype.updatePostponedAt";
const POSTPONE_MS = 24 * 60 * 60 * 1000; // 1 天

function isUpdateSuppressed(version: string): boolean {
  if (localStorage.getItem(SKIP_KEY) === version) return true;
  const at = Number(localStorage.getItem(POSTPONE_KEY) || 0);
  return at > 0 && Date.now() - at < POSTPONE_MS;
}

// 单选圆点
function Radio({ sel }: { sel: boolean }) {
  return (
    <span
      className={cn(
        "grid h-[18px] w-[18px] shrink-0 place-items-center rounded-full transition",
        sel ? "bg-[var(--accent)]" : "border border-[color:var(--control-border)]",
      )}
    >
      {sel && <span className="h-1.5 w-1.5 rounded-full bg-white" />}
    </span>
  );
}

export default function App() {
  // ---- 录音 / 错误提示 ----
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState("");
  const [injectError, setInjectError] = useState("");
  const [polishWarn, setPolishWarn] = useState("");

  // ---- 热键 ----
  const [holdKey, setHoldKey] = useState("");
  const [toggleKey, setToggleKey] = useState("");
  const [recordingWhich, setRecordingWhich] = useState<"" | "hold" | "toggle">("");
  const [hotkeyError, setHotkeyError] = useState("");
  const pendingModRef = useRef(""); // 录制时按下的候选单修饰键，松开即确认（不渲染，用 ref）
  const recordingWhichRef = useRef<"" | "hold" | "toggle">("");
  recordingWhichRef.current = recordingWhich;

  // ---- 整理风格 / 麦克风 ----
  const [polishStyle, setPolishStyleState] = useState("default");
  const [mics, setMics] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");
  const [micPickerOpen, setMicPickerOpen] = useState(false);
  const micPickerOpenRef = useRef(false);
  micPickerOpenRef.current = micPickerOpen;
  const [micBands, setMicBands] = useState<number[]>(Array(N_BANDS).fill(0));
  const micBars = useMemo(
    () => Array.from({ length: WAVE_BARS }, (_, i) => micBands[Math.abs(i - (WAVE_BARS - 1) / 2)] ?? 0),
    [micBands],
  );
  const micOptions = useMemo(
    () => [{ v: "", n: "默认设备" }, ...mics.map((m) => ({ v: m, n: m }))],
    [mics],
  );

  // ---- ASR 引擎 ----
  const [asrEngine, setAsrEngine] = useState("local");
  const [cloudVendor, setCloudVendor] = useState("volc");
  const [volcApiKey, setVolcApiKey] = useState("");
  const [volcResourceId, setVolcResourceId] = useState("");
  const [aliApiKey, setAliApiKey] = useState("");
  const [aliModel, setAliModel] = useState("qwen3");
  const [asrMsg, setAsrMsg] = useState("");
  const [asrMsgWarn, setAsrMsgWarn] = useState(false);
  const [savedEngine, setSavedEngine] = useState("local");
  const [savedVendor, setSavedVendor] = useState("volc");
  const [savedVolcKey, setSavedVolcKey] = useState("");
  const [savedAliKey, setSavedAliKey] = useState("");

  const currentEngineLabel =
    savedEngine === "cloud"
      ? savedVendor === "ali"
        ? "云端 · 阿里 Qwen3"
        : "云端 · 火山豆包"
      : "本地 SenseVoice";
  const asrUnsaved =
    asrEngine !== savedEngine ||
    (asrEngine === "cloud" &&
      (cloudVendor !== savedVendor ||
        (cloudVendor === "ali" ? aliApiKey !== savedAliKey : volcApiKey !== savedVolcKey)));

  // ---- 导航 / 引导 / 权限 ----
  const [onboarding, setOnboarding] = useState(false);
  const [section, setSection] = useState<Section>("voice");
  const [accessibilityOk, setAccessibilityOk] = useState(false);
  const accessibilityAtLaunchRef = useRef<boolean | null>(null);
  const [accessibilityAtLaunch, setAccessibilityAtLaunch] = useState<boolean | null>(null);
  const accessibilityNeedsRestart = accessibilityAtLaunch === false && accessibilityOk;

  // ---- LLM 设置 ----
  const [providerId, setProviderId] = useState("zhipu");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [disableThinking, setDisableThinking] = useState(true);
  const [saveMsg, setSaveMsg] = useState("");
  const [saveOk, setSaveOk] = useState(false);
  const keysMapRef = useRef<Record<string, string>>({});

  const currentProvider = PROVIDERS.find((p) => p.id === providerId);

  // ---- 词典 ----
  const [vocab, setVocab] = useState<VocabData>({ soft_terms: [], pinyin_terms: [], replacements: [] });
  const [vocabMsg, setVocabMsg] = useState("");
  const [newPinyin, setNewPinyin] = useState("");
  const [newSoft, setNewSoft] = useState("");
  const [newWrong, setNewWrong] = useState("");
  const [newRight, setNewRight] = useState("");
  const [suggestions, setSuggestions] = useState<LearnedPair[]>([]);

  // ---- 更新 ----
  const updater = useUpdater();
  const [updateOpen, setUpdateOpen] = useState(false);
  const [manualNoUpdate, setManualNoUpdate] = useState(false); // 手动检查后「已是最新」
  const [previewState, setPreviewState] = useState<UpdateState | null>(null); // dev 预览用

  // ============================================================
  // 供应商选择
  // ============================================================
  function selectProvider(id: string) {
    setProviderId(id);
    const p = PROVIDERS.find((x) => x.id === id);
    if (p && p.id !== "custom") {
      setBaseUrl(p.baseUrl);
      setModel(p.models[0] ?? "");
      setDisableThinking(p.disableThinking);
      setApiKey(keysMapRef.current[p.baseUrl] ?? "");
    }
  }

  const loadSettings = useCallback(async () => {
    const s = await ipc.getLlmSettings();
    if (s.base_url) {
      setBaseUrl(s.base_url);
      setApiKey(s.api_key);
      setModel(s.model);
      setDisableThinking(s.disable_thinking);
      const km = { ...(s.keys ?? {}) };
      km[s.base_url] = s.api_key;
      keysMapRef.current = km;
      const match = PROVIDERS.find((p) => p.baseUrl === s.base_url);
      setProviderId(match ? match.id : "custom");
    } else {
      keysMapRef.current = s.keys ?? {};
      selectProvider("zhipu");
    }
  }, []);

  async function saveSettings() {
    if (!apiKey || !baseUrl || !model) {
      setSaveOk(false);
      setSaveMsg("请先填好 API Key、Base URL 和模型");
      return;
    }
    setSaveOk(false);
    setSaveMsg("保存中…");
    try {
      keysMapRef.current[baseUrl] = apiKey;
      const cfg: LlmSettings = {
        base_url: baseUrl,
        api_key: apiKey,
        model,
        disable_thinking: disableThinking,
        keys: keysMapRef.current,
      };
      await ipc.setLlmSettings(cfg);
      setSaveOk(true);
      setSaveMsg("已保存，下次说话即生效");
      if (baseUrl === ALI_BASE) setAliApiKey(apiKey);
      setTimeout(() => {
        setSaveMsg("");
        setSaveOk(false);
      }, 2500);
    } catch (e) {
      setSaveOk(false);
      setSaveMsg("保存失败：" + e);
    }
  }

  // ============================================================
  // 辅助功能
  // ============================================================
  const refreshAccessibility = useCallback(async () => {
    const ok = await ipc.checkAccessibility();
    if (accessibilityAtLaunchRef.current === null) {
      accessibilityAtLaunchRef.current = ok;
      setAccessibilityAtLaunch(ok);
    }
    setAccessibilityOk(ok);
  }, []);
  async function grantAccessibility() {
    await ipc.resetAccessibilityTcc();
    await ipc.requestAccessibility();
    setTimeout(refreshAccessibility, 800);
  }
  async function restartApp() {
    await ipc.restartApp();
  }
  async function finishOnboarding(target: "voice" | "ai" = "voice") {
    await ipc.completeOnboarding();
    setOnboarding(false);
    setSection(target);
  }

  // ============================================================
  // 词典
  // ============================================================
  const loadVocab = useCallback(async () => {
    setVocab(await ipc.getVocab());
  }, []);
  const saveVocab = useCallback(async (data: VocabData) => {
    setVocabMsg("保存中…");
    try {
      await ipc.setVocab(data);
      setVocabMsg("已保存，下次说话即生效");
    } catch (e) {
      setVocabMsg("保存失败：" + e);
    }
  }, []);
  function addPinyin() {
    const t = newPinyin.trim();
    setNewPinyin("");
    if (!t || vocab.pinyin_terms.includes(t)) return;
    const next = { ...vocab, pinyin_terms: [...vocab.pinyin_terms, t] };
    setVocab(next);
    saveVocab(next);
  }
  function removePinyin(i: number) {
    const next = { ...vocab, pinyin_terms: vocab.pinyin_terms.filter((_, idx) => idx !== i) };
    setVocab(next);
    saveVocab(next);
  }
  function addSoft() {
    const t = newSoft.trim();
    setNewSoft("");
    if (!t || vocab.soft_terms.includes(t)) return;
    const next = { ...vocab, soft_terms: [...vocab.soft_terms, t] };
    setVocab(next);
    saveVocab(next);
  }
  function removeSoft(i: number) {
    const next = { ...vocab, soft_terms: vocab.soft_terms.filter((_, idx) => idx !== i) };
    setVocab(next);
    saveVocab(next);
  }
  function addReplacement() {
    const wrong = newWrong.trim();
    const right = newRight.trim();
    if (!wrong || !right) return;
    setNewWrong("");
    setNewRight("");
    const idx = vocab.replacements.findIndex((r) => r.wrong === wrong);
    let replacements: Replacement[];
    if (idx >= 0) {
      replacements = vocab.replacements.map((r, i) => (i === idx ? { wrong, right } : r));
    } else {
      replacements = [...vocab.replacements, { wrong, right }];
    }
    const next = { ...vocab, replacements };
    setVocab(next);
    saveVocab(next);
  }
  function removeReplacement(i: number) {
    const next = { ...vocab, replacements: vocab.replacements.filter((_, idx) => idx !== i) };
    setVocab(next);
    saveVocab(next);
  }

  const loadSuggestions = useCallback(async () => {
    try {
      setSuggestions(await ipc.getLearnedSuggestions());
    } catch (e) {
      console.error(e);
    }
  }, []);
  async function acceptSuggestion(s: LearnedPair) {
    try {
      await ipc.acceptLearnedSuggestion(s.right);
      await loadVocab();
      await loadSuggestions();
      setVocabMsg(`已把「${s.right}」加为拼音纠错词`);
    } catch (e) {
      setVocabMsg("操作失败：" + e);
    }
  }
  async function dismissSuggestion(s: LearnedPair) {
    try {
      await ipc.dismissLearnedSuggestion(s.wrong, s.right);
      await loadSuggestions();
    } catch (e) {
      console.error(e);
    }
  }

  // ============================================================
  // ASR 引擎
  // ============================================================
  const loadAsrConfig = useCallback(async () => {
    try {
      const c = await ipc.getAsrConfig();
      const vendor = c.cloud_vendor || "volc";
      const vKey = c.volc_api_key || "";
      const aKey = c.ali_api_key || "";
      setCloudVendor(vendor);
      setVolcApiKey(vKey);
      setVolcResourceId(c.volc_resource_id || "");
      setAliApiKey(aKey);
      setAliModel(c.ali_model || "qwen3");
      const cloudUsable = vendor === "ali" ? !!aKey.trim() : !!vKey.trim();
      const engine = c.engine === "cloud" && cloudUsable ? "cloud" : "local";
      setAsrEngine(engine);
      setSavedEngine(engine);
      setSavedVendor(vendor);
      setSavedVolcKey(vKey);
      setSavedAliKey(aKey);
    } catch (e) {
      console.error(e);
    }
  }, []);

  const saveAsrConfig = useCallback(
    async (engineArg?: string) => {
      const engine = engineArg ?? asrEngine;
      setAsrMsgWarn(false);
      if (engine === "cloud") {
        const key = (cloudVendor === "ali" ? aliApiKey : volcApiKey).trim();
        if (!key) {
          setAsrMsg(`请先填入${cloudVendor === "ali" ? "阿里" : "火山"}的 API Key（否则仍用本地 SenseVoice）`);
          setAsrMsgWarn(true);
          return;
        }
      }
      setAsrMsg("保存中…");
      try {
        await ipc.setAsrConfig({
          engine,
          cloud_vendor: cloudVendor,
          volc_api_key: volcApiKey,
          volc_resource_id: volcResourceId,
          ali_api_key: aliApiKey,
          ali_model: aliModel,
        });
        if (aliApiKey) keysMapRef.current[ALI_BASE] = aliApiKey;
        setSavedEngine(engine);
        setSavedVendor(cloudVendor);
        setSavedVolcKey(volcApiKey);
        setSavedAliKey(aliApiKey);
        setAsrMsg("已保存，下次说话生效");
        setTimeout(() => setAsrMsg((m) => (m === "已保存，下次说话生效" ? "" : m)), 2500);
      } catch (e) {
        setAsrMsg("保存失败：" + e);
        setAsrMsgWarn(true);
      }
    },
    [asrEngine, cloudVendor, volcApiKey, volcResourceId, aliApiKey, aliModel],
  );

  function chooseAsrEngine(engine: string) {
    setAsrEngine(engine);
    setAsrMsg("");
    if (engine === "local") saveAsrConfig("local");
  }
  function resetAsrEditing() {
    setAsrEngine(savedEngine);
    setCloudVendor(savedVendor);
    setVolcApiKey(savedVolcKey);
    setAliApiKey(savedAliKey);
    setAsrMsg("");
  }
  function selectSection(id: Section) {
    if (id === "engine") resetAsrEditing();
    setSection(id);
  }

  // ============================================================
  // 整理风格 / 麦克风
  // ============================================================
  async function setPolishStyle(s: string) {
    setPolishStyleState(s);
    try {
      await ipc.setPolishStyle(s);
    } catch (e) {
      console.error(e);
    }
  }
  const setMic = useCallback(async (name: string) => {
    setSelectedMic(name);
    try {
      await ipc.setMicrophone(name);
    } catch (e) {
      console.error(e);
    }
  }, []);
  function startMicMonitor(device: string) {
    ipc.startMicMonitor(device).catch(() => {});
  }
  async function openMicPicker() {
    setMicPickerOpen(true);
    setMicBands(Array(N_BANDS).fill(0));
    try {
      setMics(await ipc.listMicrophones());
    } catch {
      // ignore
    }
    startMicMonitor(selectedMic);
  }
  const closeMicPicker = useCallback(() => {
    setMicPickerOpen(false);
    setMicBands(Array(N_BANDS).fill(0));
    ipc.stopMicMonitor().catch(() => {});
  }, []);
  async function chooseMic(name: string) {
    await setMic(name);
    setMicBands(Array(N_BANDS).fill(0));
    startMicMonitor(name);
  }

  // ============================================================
  // 检查更新
  // ============================================================
  const checkForUpdates = updater.checkForUpdates;
  // 启动静默检查：有更新且未被「跳过 / 冷却」才自动弹。
  const autoCheckUpdate = useCallback(async () => {
    const u = await checkForUpdates();
    if (u && !isUpdateSuppressed(u.version)) setUpdateOpen(true);
  }, [checkForUpdates]);
  // 关于页手动检查：有更新就弹（无视跳过/冷却）；没有则提示「已是最新」。
  const manualCheckUpdate = useCallback(async () => {
    setManualNoUpdate(false);
    const u = await checkForUpdates();
    if (u) setUpdateOpen(true);
    else setManualNoUpdate(true);
  }, [checkForUpdates]);
  // 弹窗当前展示的状态：dev 预览优先，否则真实 updater 状态
  const modalState = previewState ?? updater.state;
  const closePreview = () => setPreviewState(null);
  function laterUpdate() {
    localStorage.setItem(POSTPONE_KEY, String(Date.now()));
    setUpdateOpen(false);
    closePreview();
  }
  function skipUpdate() {
    const v = modalState.newVersion;
    if (v) localStorage.setItem(SKIP_KEY, v);
    setUpdateOpen(false);
    closePreview();
  }

  // ============================================================
  // 快捷键录制
  // ============================================================
  function startRecord(which: "hold" | "toggle") {
    if (recordingWhich === which) {
      setRecordingWhich("");
      recordingWhichRef.current = "";
      pendingModRef.current = "";
      return;
    }
    setHotkeyError("");
    pendingModRef.current = "";
    setRecordingWhich(which);
    recordingWhichRef.current = which;
  }
  const applyShortcut = useCallback(async (which: "hold" | "toggle", accel: string) => {
    try {
      await ipc.setShortcut(which, accel);
      if (which === "hold") setHoldKey(accel);
      else setToggleKey(accel);
      setHotkeyError("");
    } catch (e) {
      setHotkeyError(String(e)); // 后端拒绝（非法键）：旧键仍有效
    }
  }, []);
  function clearShortcut(which: "hold" | "toggle") {
    applyShortcut(which, "");
  }

  // window 键盘监听（挂载一次，读 ref 拿最新态）
  useEffect(() => {
    function finishRecord(accel: string) {
      const which = recordingWhichRef.current;
      setRecordingWhich("");
      recordingWhichRef.current = "";
      pendingModRef.current = "";
      if (which) applyShortcut(which, accel);
    }
    function onKeyDown(e: KeyboardEvent) {
      if (micPickerOpenRef.current) {
        if (e.key === "Escape") closeMicPicker();
        return;
      }
      if (!recordingWhichRef.current) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecordingWhich("");
        recordingWhichRef.current = "";
        pendingModRef.current = "";
        return;
      }
      if (MOD_KEYS.includes(e.key)) {
        pendingModRef.current = e.code; // 区分左右
        setHotkeyError("");
        return;
      }
      const key = mainKeyToken(e.code);
      if (!key) {
        setHotkeyError("不支持该按键；可用单个修饰键 / 功能键，或「修饰键 + 字母/数字」组合");
        return;
      }
      const mods: string[] = [];
      if (e.metaKey) mods.push("Meta");
      if (e.ctrlKey) mods.push("Control");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");
      const isFn = /^F([1-9]|1[0-9]|2[0-4])$/.test(key);
      if (mods.length === 0 && !isFn) {
        setHotkeyError("单个字母 / 数字需配修饰键；或直接用单个修饰键、功能键");
        return;
      }
      finishRecord(mods.length ? [...mods, key].join("+") : key);
    }
    function onKeyUp(e: KeyboardEvent) {
      if (!recordingWhichRef.current || !pendingModRef.current) return;
      if (e.code === pendingModRef.current) {
        e.preventDefault();
        finishRecord(pendingModRef.current);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [applyShortcut, closeMicPicker]);

  // ============================================================
  // 初始化加载 + 事件订阅
  // ============================================================
  useEffect(() => {
    ipc.getRecording().then(setRecording);
    ipc.getOnboarded().then((done) => {
      if (!done) setOnboarding(true);
    });
    ipc.getShortcuts().then(([h, t]) => {
      setHoldKey(h);
      setToggleKey(t);
    });
    ipc.getPolishStyle().then((s) => {
      if (s) setPolishStyleState(s);
    });
    ipc.listMicrophones().then(setMics);
    ipc.getMicrophone().then(setSelectedMic);
    loadSettings();
    loadVocab();
    loadAsrConfig();
    refreshAccessibility();
  }, [loadSettings, loadVocab, loadAsrConfig, refreshAccessibility]);

  // 启动后延迟 ~3s 静默检查更新（不抢冷启动首帧）
  useEffect(() => {
    const t = setTimeout(autoCheckUpdate, 3000);
    return () => clearTimeout(t);
  }, [autoCheckUpdate]);

  // 辅助功能：定时 + 窗口聚焦时刷新
  useEffect(() => {
    const t = setInterval(refreshAccessibility, 2000);
    const onFocus = () => refreshAccessibility();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", onFocus);
    };
  }, [refreshAccessibility]);

  // 进入「词典」页时刷新建议
  useEffect(() => {
    if (section === "dict") loadSuggestions();
  }, [section, loadSuggestions]);

  // 事件订阅
  useTauriEvent<string>("recording-state", (payload) => {
    const rec = payload === "recording";
    setRecording(rec);
    if (rec) {
      setError("");
      setInjectError("");
      setPolishWarn("");
    }
  });
  useTauriEvent<string>("asr-error", (p) => setError(p));
  useTauriEvent<string>("inject-error", (p) => setInjectError(p));
  useTauriEvent("llm-error", () => setPolishWarn("AI 整理失败，已输出原始识别稿"));
  useTauriEvent<number[]>("mic-level", (incoming) => {
    setMicBands((prev) =>
      prev.map((cur, i) => {
        const t = incoming[i] ?? 0;
        return cur + (t - cur) * (t > cur ? 0.6 : 0.28); // 快起 / 慢落
      }),
    );
  });

  // ============================================================
  // 渲染
  // ============================================================
  if (onboarding) {
    return (
      <main className="flex h-screen flex-col bg-[var(--bg)] text-[color:var(--fg)]">
        <div data-tauri-drag-region className="h-10 shrink-0" />
        <div className="flex flex-1 items-center justify-center overflow-y-auto px-6 pb-8">
          <div className="w-full max-w-md space-y-5">
            <div>
              <h2 className="flex items-center gap-2 text-lg font-semibold">
                <Mic size={20} className="text-[color:var(--accent)]" /> 欢迎使用 Untype
                <span className="text-[color:var(--accent)]">.</span>
              </h2>
              <p className="mt-1 text-sm leading-relaxed text-[color:var(--fg-2)]">
                按{" "}
                <kbd className="rounded border border-[color:var(--hairline)] bg-[var(--kbd-bg)] px-1.5 py-0.5 font-mono">
                  {prettyShortcut(holdKey || toggleKey)}
                </kbd>{" "}
                说话，松开后自动转成文字、AI 整理，并注入到光标处。
              </p>
            </div>

            <div className="card p-4">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">① 辅助功能权限（推荐）</span>
                {accessibilityOk ? (
                  <span className="inline-flex items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
                    <Check size={14} /> 已授权
                  </span>
                ) : (
                  <button
                    className="rounded-md bg-[var(--accent)] px-3 py-1 text-xs font-medium text-white transition hover:bg-[var(--accent-hover)]"
                    onClick={grantAccessibility}
                  >
                    开启
                  </button>
                )}
              </div>
              <p className="mt-1 text-xs leading-relaxed text-[color:var(--fg-2)]">
                用于把文字自动注入到光标。不授也能用——会自动复制到剪贴板，⌘V 粘贴。
              </p>
            </div>

            <div className="card p-4">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">② AI 整理（可选）</span>
                <button
                  className="rounded-md border border-[color:var(--control-border)] bg-[var(--control)] px-3 py-1 text-xs font-medium transition hover:bg-[var(--control-hover)]"
                  onClick={() => finishOnboarding("ai")}
                >
                  去配置
                </button>
              </div>
              <p className="mt-1 text-xs leading-relaxed text-[color:var(--fg-2)]">
                配个云模型（有免费档），自动去口水词、分点、纠正同音字。不配也能用，只是输出原始识别文本。
              </p>
            </div>

            <p className="flex items-start gap-1.5 text-xs text-[color:var(--fg-3)]">
              <Info size={13} className="mt-0.5 shrink-0" />
              <span>麦克风权限会在你第一次说话时由系统自动弹窗请求，点「允许」即可。</span>
            </p>

            <button
              className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-[var(--accent)] py-2.5 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
              onClick={() => finishOnboarding("voice")}
            >
              开始使用 <ArrowRight size={15} />
            </button>
          </div>
        </div>
      </main>
    );
  }

  return (
    <>
      <main className="flex h-screen bg-[var(--bg)] text-[color:var(--fg)]">
        {/* 侧栏 */}
        <aside className="flex w-[200px] shrink-0 flex-col border-r border-[color:var(--border)] px-3">
          <div data-tauri-drag-region className="h-10 shrink-0" />
          <nav className="flex flex-col gap-1">
            {NAV.map((item) => {
              const Icon = item.icon;
              return (
                <button
                  key={item.id}
                  className={cn(
                    "flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-sm font-medium transition",
                    section === item.id
                      ? "bg-[var(--accent)] text-white shadow-sm"
                      : "text-[color:var(--fg-2)] hover:bg-[var(--surface-hover)] hover:text-[color:var(--fg)]",
                  )}
                  onClick={() => selectSection(item.id)}
                >
                  <Icon size={16} />
                  {item.label}
                </button>
              );
            })}
          </nav>
        </aside>

        {/* 内容 */}
        <section className="flex flex-1 flex-col overflow-hidden">
          <div data-tauri-drag-region className="h-10 shrink-0" />
          <div className="flex shrink-0 items-center justify-between gap-3 px-7 pb-5">
            <h1 className="text-[17px] font-semibold tracking-tight">{SECTION_TITLE[section]}</h1>
            {section === "voice" && (
              <span className="status-pill">
                <span className={cn("led", recording && "on animate-pulse")} />
                {recording ? "录音中…" : "空闲"}
              </span>
            )}
          </div>
          <div className="flex-1 overflow-y-auto px-7 pb-12">
            <motion.div
              key={section}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.16 }}
            >
              {section === "voice" && (
                <>
                  {/* 快捷键 */}
                  <div className="card">
                    <div className="card-head">快捷键</div>
                    {!accessibilityOk ? (
                      <div className="mx-4 mb-1 flex items-center gap-2.5 rounded-md bg-amber-50 px-2.5 py-1.5 text-xs leading-snug text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
                        <span className="min-w-0 flex-1">
                          单键热键需要「辅助功能」权限，点「开启」去系统设置授权。
                        </span>
                        <button
                          className="shrink-0 rounded bg-amber-600/90 px-2.5 py-1 font-medium text-white transition hover:bg-amber-600 dark:bg-amber-500/90 dark:hover:bg-amber-500"
                          onClick={grantAccessibility}
                        >
                          开启
                        </button>
                      </div>
                    ) : accessibilityNeedsRestart ? (
                      <div className="mx-4 mb-1 flex items-center gap-2.5 rounded-md bg-emerald-50 px-2.5 py-1.5 text-xs leading-snug text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300">
                        <span className="min-w-0 flex-1">✓ 辅助功能已授权，重启 app 后单键热键即生效。</span>
                        <button
                          className="shrink-0 rounded bg-emerald-600 px-2.5 py-1 font-medium text-white transition hover:bg-emerald-700 dark:bg-emerald-600 dark:hover:bg-emerald-500"
                          onClick={restartApp}
                        >
                          立即重启
                        </button>
                      </div>
                    ) : null}
                    <div className="divide-y divide-[color:var(--hairline)]">
                      {KEY_ROWS.map((row) => {
                        const key = row.which === "hold" ? holdKey : toggleKey;
                        const rec = recordingWhich === row.which;
                        return (
                          <div className="ui-row" key={row.which}>
                            <div className="min-w-0 flex-1">
                              <div className="text-sm font-medium">{row.title}</div>
                              <div className="desc">{row.desc}</div>
                            </div>
                            <div className="flex shrink-0 items-center gap-1.5">
                              <button
                                className={cn("kbd", rec && "recording")}
                                onClick={() => startRecord(row.which)}
                              >
                                {rec ? "按住要绑的键…" : prettyShortcut(key)}
                              </button>
                              <button
                                className="btn-del"
                                title={rec ? "取消" : "清除"}
                                aria-label="清除该热键"
                                disabled={!key && !rec}
                                onClick={() =>
                                  rec ? setRecordingWhich("") : clearShortcut(row.which)
                                }
                              >
                                <X size={14} />
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                    {hotkeyError && <p className="px-4 pt-2 text-xs text-red-500">{hotkeyError}</p>}
                    <p className="px-4 pt-1 pb-3 text-xs leading-relaxed text-[color:var(--fg-3)]">
                      点徽章后按要绑的键：可单个修饰键（如 右⌥）、功能键，或「修饰键 +
                      字母/数字」组合；免按为双击触发。两个模式可同时启用，✕ 清除即停用，Esc 取消。
                    </p>
                  </div>

                  {/* 麦克风 */}
                  <div className="card mt-4">
                    <button className="ui-row justify-between tap" onClick={openMicPicker}>
                      <div className="text-sm font-medium">麦克风</div>
                      <span className="flex min-w-0 items-center gap-1.5 text-sm text-[color:var(--fg-2)]">
                        <span className="truncate">{selectedMic || "默认设备"}</span>
                        <ChevronDown size={14} className="shrink-0 text-[color:var(--fg-3)]" />
                      </span>
                    </button>
                  </div>

                  {/* 整理风格 */}
                  <div className="card mt-4">
                    <div className="card-head">整理风格</div>
                    <div className="divide-y divide-[color:var(--hairline)]">
                      {STYLES.map((s) => (
                        <button
                          key={s.id}
                          className="ui-row tap"
                          onClick={() => setPolishStyle(s.id)}
                        >
                          <Radio sel={polishStyle === s.id} />
                          <div className="min-w-0 flex-1">
                            <div className="text-sm font-medium">{s.title}</div>
                            <div className="desc">{s.desc}</div>
                          </div>
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* 错误提示 */}
                  {(error || injectError || polishWarn) && (
                    <div className="card mt-4">
                      <div className="space-y-2 px-4 py-3">
                        {error && <p className="text-sm text-red-500">识别出错：{error}</p>}
                        {polishWarn && (
                          <p className="text-xs text-amber-600 dark:text-amber-400">{polishWarn}</p>
                        )}
                        {injectError && (
                          <p className="flex items-start gap-1.5 text-xs text-amber-600 dark:text-amber-400">
                            <TriangleAlert size={14} className="mt-0.5 shrink-0" />
                            <span>
                              已识别但无法自动注入——文本已复制到剪贴板，按{" "}
                              <kbd className="rounded bg-amber-100 px-1 dark:bg-amber-900">⌘V</kbd>{" "}
                              粘贴即可。如需自动注入，请到「系统设置 → 隐私与安全性 →
                              辅助功能」给本 app 授权。
                            </span>
                          </p>
                        )}
                      </div>
                    </div>
                  )}
                </>
              )}

              {section === "engine" && (
                <>
                  <div
                    className={cn(
                      "mb-4 flex items-center gap-2 rounded-lg border px-3 py-2 text-sm",
                      asrMsg && asrMsgWarn
                        ? "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-800/60 dark:bg-amber-950/30 dark:text-amber-300"
                        : "border-green-300 bg-green-50 text-green-800 dark:border-green-800/60 dark:bg-green-950/30 dark:text-green-300",
                    )}
                  >
                    {asrMsg && asrMsgWarn ? (
                      <TriangleAlert size={15} className="shrink-0" />
                    ) : (
                      <Check size={15} className="shrink-0" />
                    )}
                    <span>
                      当前使用 <span className="font-semibold">{currentEngineLabel}</span>
                      {asrMsg ? ` · ${asrMsg}` : ""}
                    </span>
                  </div>

                  <div className="card">
                    <div className="card-head">识别引擎</div>
                    <div className="divide-y divide-[color:var(--hairline)]">
                      <button className="ui-row tap" onClick={() => chooseAsrEngine("local")}>
                        <Radio sel={asrEngine === "local"} />
                        <div className="min-w-0 flex-1">
                          <div className="text-sm font-medium">本地 SenseVoice</div>
                          <div className="desc">离线 · 免费 · 隐私；自带标点，中英混说 OK</div>
                        </div>
                      </button>
                      <button className="ui-row tap" onClick={() => chooseAsrEngine("cloud")}>
                        <Radio sel={asrEngine === "cloud"} />
                        <div className="min-w-0 flex-1">
                          <div className="text-sm font-medium">云端（火山 / 阿里）</div>
                          <div className="desc">中英混说 / 技术词 / 标点更强；音频上云、按量计费</div>
                        </div>
                      </button>
                    </div>
                    {asrEngine === "cloud" && (
                      <div className="space-y-3 p-4 pt-3">
                        <div className="seg w-full">
                          {[
                            { id: "volc", name: "火山豆包" },
                            { id: "ali", name: "阿里 Qwen3" },
                          ].map((v) => (
                            <button
                              key={v.id}
                              className={cn(cloudVendor === v.id && "on")}
                              onClick={() => {
                                setCloudVendor(v.id);
                                setAsrMsg("");
                              }}
                            >
                              {v.name}
                            </button>
                          ))}
                        </div>

                        {cloudVendor === "volc" ? (
                          <>
                            <label className="block text-sm font-medium">
                              火山 API Key
                              <input
                                className="input mt-1.5"
                                type="password"
                                value={volcApiKey}
                                onChange={(e) => setVolcApiKey(e.target.value)}
                                placeholder="火山控制台 → 语音技术 → API Key"
                              />
                            </label>
                            <p className="flex items-start gap-1.5 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                              <Info size={13} className="mt-0.5 shrink-0" />
                              <span>
                                去{" "}
                                <button
                                  className="text-[color:var(--accent)] hover:underline"
                                  onClick={() =>
                                    openUrl("https://console.volcengine.com/speech/new/setting/apikeys")
                                  }
                                >
                                  火山控制台
                                </button>{" "}
                                开通「流式语音识别大模型」取
                                Key（有免费试用额度，以控制台为准）；支持热词偏置（吃你的词典）。
                              </span>
                            </p>
                          </>
                        ) : (
                          <>
                            <label className="block text-sm font-medium">
                              阿里 DashScope API Key
                              <input
                                className="input mt-1.5"
                                type="password"
                                value={aliApiKey}
                                onChange={(e) => setAliApiKey(e.target.value)}
                                placeholder="百炼控制台 → API-KEY"
                              />
                            </label>
                            <p className="flex items-start gap-1.5 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                              <Info size={13} className="mt-0.5 shrink-0" />
                              <span>
                                去{" "}
                                <button
                                  className="text-[color:var(--accent)] hover:underline"
                                  onClick={() => openUrl("https://bailian.console.aliyun.com/")}
                                >
                                  百炼控制台
                                </button>{" "}
                                创建 API-KEY（新用户有免费额度）。用 Qwen3-ASR-Flash（大模型 ASR，中英
                                / 技术词更强）；暂不带内联热词，靠模型 + 词典纠错。
                              </span>
                            </p>
                          </>
                        )}

                        <p className="text-xs text-neutral-400 dark:text-neutral-500">
                          音频会上传到对应云端。
                        </p>

                        <div className="flex items-center gap-3 pt-1">
                          <button
                            className="rounded-lg bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
                            onClick={() => saveAsrConfig()}
                          >
                            保存
                          </button>
                          {asrUnsaved ? (
                            <span className="text-xs text-amber-600 dark:text-amber-400">
                              未保存 · 当前仍按「{currentEngineLabel}」运行，点保存生效
                            </span>
                          ) : (
                            <span className="text-xs text-neutral-400 dark:text-neutral-500">
                              已保存，下次说话即用此引擎
                            </span>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                </>
              )}

              {section === "ai" && (
                <>
                  <p className="lead">
                    选一个云端整理模型（OpenAI 兼容）。标「免费」的可零成本用；Key 只存在本机。
                  </p>

                  <div className="card">
                    <div className="card-head">供应商</div>
                    <div className="grid grid-cols-2 gap-2.5 px-4 pb-4">
                      {PROVIDERS.map((p) => (
                        <button
                          key={p.id}
                          className={cn(
                            "flex flex-col items-start gap-1 rounded-lg border p-3 text-left transition",
                            providerId === p.id
                              ? "border-[color:var(--accent)] bg-[var(--accent-soft)]"
                              : "border-[color:var(--border)] hover:border-[color:var(--fg-3)]",
                          )}
                          onClick={() => selectProvider(p.id)}
                        >
                          <span className="flex items-center gap-1.5 text-sm font-medium">
                            {p.name}
                            {p.free && (
                              <span className="rounded bg-green-100 px-1.5 py-0.5 text-[10px] font-medium text-green-700 dark:bg-green-900/60 dark:text-green-300">
                                免费
                              </span>
                            )}
                          </span>
                          <span className="text-xs leading-snug text-neutral-500 dark:text-neutral-400">
                            {p.note}
                          </span>
                        </button>
                      ))}
                    </div>
                    {currentProvider?.register && (
                      <button
                        className="inline-flex items-center gap-1 px-4 pb-4 text-sm text-[color:var(--accent)] hover:underline"
                        onClick={() => openUrl(currentProvider.register)}
                      >
                        去 {currentProvider.name} 获取 API Key <ExternalLink size={13} />
                      </button>
                    )}
                  </div>

                  <div className="card mt-4 space-y-4 p-4">
                    <label className="block text-sm font-medium">
                      API Key
                      <input
                        className="input mt-1.5"
                        type="password"
                        value={apiKey}
                        onChange={(e) => setApiKey(e.target.value)}
                        placeholder="粘贴你的 Key"
                      />
                    </label>

                    <div>
                      <p className="mb-1.5 text-sm font-medium">模型</p>
                      {(currentProvider?.models ?? []).length > 0 && (
                        <div className="mb-2 flex flex-wrap gap-1.5">
                          {(currentProvider?.models ?? []).map((m) => (
                            <button
                              key={m}
                              className={cn(
                                "rounded-full border px-2.5 py-1 text-xs transition",
                                model === m
                                  ? "border-[color:var(--accent)] bg-[var(--accent-soft)] text-[color:var(--accent)]"
                                  : "border-[color:var(--border)] hover:border-[color:var(--fg-3)]",
                              )}
                              onClick={() => setModel(m)}
                            >
                              {m}
                            </button>
                          ))}
                        </div>
                      )}
                      <input
                        className="input"
                        value={model}
                        onChange={(e) => setModel(e.target.value)}
                        placeholder="或手填模型名"
                      />
                    </div>

                    <label className="block text-sm font-medium">
                      Base URL
                      <input
                        className="input mt-1.5"
                        value={baseUrl}
                        onChange={(e) => setBaseUrl(e.target.value)}
                        placeholder="https://.../v1"
                      />
                    </label>

                    <div className="flex items-center gap-3 pt-1">
                      <button
                        className="rounded-lg bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
                        onClick={saveSettings}
                      >
                        保存
                      </button>
                      {saveMsg && (
                        <span
                          className={cn(
                            "text-sm",
                            saveOk
                              ? "text-green-600 dark:text-green-400"
                              : "text-amber-600 dark:text-amber-400",
                          )}
                        >
                          {saveMsg}
                        </span>
                      )}
                    </div>
                  </div>
                </>
              )}

              {section === "dict" && (
                <>
                  <p className="mb-1 text-sm leading-relaxed text-neutral-500 dark:text-neutral-400">
                    教 app 认准你的常用词、纠正同音误识。改完即时生效，下次说话就用上。
                  </p>
                  <p className="mb-4 h-4 text-xs text-green-600 dark:text-green-400">{vocabMsg}</p>

                  {suggestions.length > 0 && (
                    <div className="card mb-4 border-[color:var(--accent)]">
                      <div className="card-head">建议 · 自动学习</div>
                      <p className="px-4 pb-2 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                        这些词常被 AI
                        纠正。加为拼音纠错词后会直接确定性纠对（一条顶所有同音变体），越用越准。
                      </p>
                      <div className="divide-y divide-[color:var(--hairline)]">
                        {suggestions.map((s) => (
                          <div className="ui-row justify-between gap-2" key={s.wrong + "→" + s.right}>
                            <span className="inline-flex min-w-0 items-center gap-1.5 text-sm">
                              <span className="truncate text-neutral-400 line-through">{s.wrong}</span>
                              <ArrowRight size={13} className="shrink-0 text-neutral-400" />
                              <span className="truncate font-medium">{s.right}</span>
                              <span className="shrink-0 text-xs text-neutral-400">×{s.count}</span>
                            </span>
                            <span className="flex shrink-0 items-center gap-1.5">
                              <button
                                className="rounded-md bg-[var(--accent)] px-2.5 py-1 text-xs font-medium text-white transition hover:bg-[var(--accent-hover)]"
                                onClick={() => acceptSuggestion(s)}
                              >
                                加入
                              </button>
                              <button
                                className="btn-del"
                                onClick={() => dismissSuggestion(s)}
                                aria-label="忽略"
                              >
                                <X size={14} />
                              </button>
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* 拼音纠错词 */}
                  <div className="card">
                    <div className="card-head">拼音纠错词</div>
                    <p className="px-4 pb-3 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                      读音相同就强制纠成它，一条顶所有同音变体（如「中文强」纠掉「中文墙 /
                      中文枪」）。适合常被同音听错、自身又不是日常常用词的词。
                    </p>
                    <div className="flex flex-wrap gap-2 px-4">
                      {vocab.pinyin_terms.map((t, i) => (
                        <span
                          key={t + i}
                          className="inline-flex items-center gap-1 rounded-full border border-[color:var(--accent)] bg-[var(--accent-soft)] py-1 pl-3 pr-1.5 text-sm text-[color:var(--accent)]"
                        >
                          {t}
                          <button
                            className="grid h-5 w-5 place-items-center rounded-full text-[color:var(--accent)] opacity-70 transition hover:bg-[var(--accent-soft)] hover:opacity-100"
                            onClick={() => removePinyin(i)}
                            aria-label="删除"
                          >
                            <X size={12} />
                          </button>
                        </span>
                      ))}
                      {!vocab.pinyin_terms.length && (
                        <span className="text-xs text-neutral-400">还没有，下面添加</span>
                      )}
                    </div>
                    <div className="flex items-center gap-2 p-4">
                      <input
                        className="input"
                        placeholder="正确写法，如 中文强"
                        value={newPinyin}
                        onChange={(e) => setNewPinyin(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && addPinyin()}
                      />
                      <button
                        className="btn-add"
                        onClick={addPinyin}
                        disabled={!newPinyin.trim()}
                        aria-label="添加"
                      >
                        <Plus size={16} />
                      </button>
                    </div>
                  </div>

                  {/* 软术语 */}
                  <div className="card mt-4">
                    <div className="card-head">软术语</div>
                    <p className="px-4 pb-3 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                      只交给 AI 参考、结合上下文判断，不强制替换。适合「优化 /
                      流式」这类本身常用、强纠会误伤的词。
                    </p>
                    <div className="flex flex-wrap gap-2 px-4">
                      {vocab.soft_terms.map((t, i) => (
                        <span
                          key={t + i}
                          className="inline-flex items-center gap-1 rounded-full border border-[color:var(--border)] bg-[var(--surface-hover)] py-1 pl-3 pr-1.5 text-sm"
                        >
                          {t}
                          <button
                            className="grid h-5 w-5 place-items-center rounded-full text-[color:var(--fg-3)] transition hover:text-[color:var(--fg)]"
                            onClick={() => removeSoft(i)}
                            aria-label="删除"
                          >
                            <X size={12} />
                          </button>
                        </span>
                      ))}
                      {!vocab.soft_terms.length && (
                        <span className="text-xs text-neutral-400">还没有，下面添加</span>
                      )}
                    </div>
                    <div className="flex items-center gap-2 p-4">
                      <input
                        className="input"
                        placeholder="术语正确写法，如 SenseVoice"
                        value={newSoft}
                        onChange={(e) => setNewSoft(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && addSoft()}
                      />
                      <button
                        className="btn-add"
                        onClick={addSoft}
                        disabled={!newSoft.trim()}
                        aria-label="添加"
                      >
                        <Plus size={16} />
                      </button>
                    </div>
                  </div>

                  {/* 硬替换 */}
                  <div className="card mt-4">
                    <div className="card-head">硬替换</div>
                    <p className="px-4 pb-2 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                      把固定错字整串换成对的（如「停路 →
                      停录」）。只放几乎总错成同一写法、且不会误伤的。
                    </p>
                    <div className="divide-y divide-[color:var(--hairline)]">
                      {vocab.replacements.map((r, i) => (
                        <div className="ui-row justify-between" key={r.wrong + i}>
                          <span className="inline-flex min-w-0 items-center gap-1.5 text-sm">
                            <span className="truncate text-neutral-400 line-through">{r.wrong}</span>
                            <ArrowRight size={13} className="shrink-0 text-neutral-400" />
                            <span className="truncate font-medium">{r.right}</span>
                          </span>
                          <button
                            className="btn-del"
                            onClick={() => removeReplacement(i)}
                            aria-label="删除"
                          >
                            <X size={14} />
                          </button>
                        </div>
                      ))}
                      {!vocab.replacements.length && (
                        <div className="px-4 py-3 text-xs text-neutral-400">还没有，下面添加</div>
                      )}
                    </div>
                    <div className="flex items-center gap-2 p-4">
                      <input
                        className="input"
                        placeholder="误识"
                        value={newWrong}
                        onChange={(e) => setNewWrong(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && addReplacement()}
                      />
                      <ArrowRight size={16} className="shrink-0 text-neutral-400" />
                      <input
                        className="input"
                        placeholder="正确"
                        value={newRight}
                        onChange={(e) => setNewRight(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && addReplacement()}
                      />
                      <button
                        className="btn-add"
                        onClick={addReplacement}
                        disabled={!newWrong.trim() || !newRight.trim()}
                        aria-label="添加"
                      >
                        <Plus size={16} />
                      </button>
                    </div>
                  </div>
                </>
              )}

              {section === "about" && (
                <>
                  <div className="mb-5 flex flex-col items-center gap-2 text-center">
                    {/* Untype LOGO：声波 → 基线 → 文字光标的连续笔画 */}
                    <svg viewBox="200 366 612 292" className="h-12 w-auto" fill="none" aria-hidden="true">
                      <path
                        d="M 226 512 C 256 384, 300 384, 330 512 C 360 640, 404 640, 432 512 C 456 424, 494 446, 520 512 C 540 558, 562 530, 582 512 L 742 512"
                        stroke="var(--fg)"
                        strokeWidth="40"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                      <rect x="752" y="404" width="42" height="216" rx="21" fill="var(--accent)" />
                    </svg>
                    <div className="text-2xl font-semibold tracking-tight">
                      Untype<span className="text-[color:var(--accent)]">.</span>
                    </div>
                    <div className="text-xs text-[color:var(--fg-3)]">
                      语音听写 · v{updater.state.currentVersion || "0.1.0"}
                    </div>
                    <div className="text-xs">
                      {updater.state.isChecking ? (
                        <span className="text-[color:var(--fg-3)]">检查更新中…</span>
                      ) : updater.state.hasUpdate ? (
                        <button
                          className="text-[color:var(--accent)] hover:underline"
                          onClick={() => setUpdateOpen(true)}
                        >
                          有新版本 v{updater.state.newVersion} · 查看
                        </button>
                      ) : manualNoUpdate ? (
                        <>
                          <span className="text-[color:var(--fg-3)]">已是最新</span>
                          <button
                            className="ml-1 text-[color:var(--fg-3)] underline hover:text-[color:var(--fg-2)]"
                            onClick={manualCheckUpdate}
                          >
                            重新检查
                          </button>
                        </>
                      ) : (
                        <button
                          className="text-[color:var(--accent)] hover:underline"
                          onClick={manualCheckUpdate}
                        >
                          检查更新
                        </button>
                      )}
                    </div>
                    {/* dev-only：无需真实新版本即可预览弹窗（生产构建不含） */}
                    {import.meta.env.DEV && (
                      <button
                        className="text-[11px] text-[color:var(--fg-3)] underline hover:text-[color:var(--fg-2)]"
                        onClick={() => {
                          setPreviewState({
                            isChecking: false,
                            hasUpdate: true,
                            isDownloading: false,
                            isInstalling: false,
                            isRestarting: false,
                            requiresManualRestart: false,
                            downloadProgress: 0,
                            error: null,
                            currentVersion: updater.state.currentVersion || "0.1.0",
                            newVersion: "0.2.0",
                            notes:
                              "• 全新 React 前端，动画更顺滑\n• 应用内自动更新（下载/安装/重启）\n• 词典自学建议优化\n• 修复若干已知问题",
                          });
                          setUpdateOpen(true);
                        }}
                      >
                        预览更新弹窗(dev)
                      </button>
                    )}
                    <p className="max-w-xs text-sm leading-relaxed text-[color:var(--fg-2)]">
                      说话，不用打字——本地 SenseVoice 识别 + 云端 AI
                      轻整理，按住热键说话即转成文字、注入光标。
                    </p>
                  </div>

                  <div className="card">
                    <div className="card-head">权限</div>
                    <div className="divide-y divide-[color:var(--hairline)]">
                      <div className="ui-row">
                        <div className="min-w-0 flex-1">
                          <div className="text-sm font-medium">辅助功能</div>
                          <div className="desc">把文字自动注入光标；未授权则复制到剪贴板，⌘V 粘贴。</div>
                        </div>
                        {accessibilityNeedsRestart ? (
                          <button
                            className="shrink-0 rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white transition hover:bg-emerald-700"
                            onClick={restartApp}
                          >
                            重启生效
                          </button>
                        ) : accessibilityOk ? (
                          <span className="inline-flex shrink-0 items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
                            <Check size={14} /> 已授权
                          </span>
                        ) : (
                          <button
                            className="shrink-0 rounded-md bg-[var(--accent)] px-3 py-1 text-xs font-medium text-white transition hover:bg-[var(--accent-hover)]"
                            onClick={grantAccessibility}
                          >
                            开启
                          </button>
                        )}
                      </div>
                      <div className="ui-row">
                        <div className="min-w-0 flex-1">
                          <div className="text-sm font-medium">麦克风</div>
                          <div className="desc">首次说话时由系统弹窗申请，点「允许」即可。</div>
                        </div>
                      </div>
                    </div>
                  </div>

                  <div className="card mt-4">
                    <div className="card-head">信息</div>
                    <div className="divide-y divide-[color:var(--hairline)]">
                      <div className="ui-row">
                        <div className="flex-1 text-sm font-medium">全局热键</div>
                        <span className="shrink-0 text-sm text-neutral-500 dark:text-neutral-400">
                          长按 {prettyShortcut(holdKey)} · 免按 {prettyShortcut(toggleKey)}
                        </span>
                      </div>
                      <div className="ui-row">
                        <div className="flex-1 text-sm font-medium">识别引擎</div>
                        <span className="shrink-0 text-sm text-neutral-500 dark:text-neutral-400">
                          {currentEngineLabel}
                        </span>
                      </div>
                      <div className="ui-row">
                        <div className="flex-1 text-sm font-medium">整理模型</div>
                        <span className="shrink-0 text-sm text-neutral-500 dark:text-neutral-400">
                          {currentProvider?.name ?? "未配置"}
                          {model ? " · " + model : ""}
                        </span>
                      </div>
                    </div>
                  </div>

                  <button
                    className="mx-auto mt-5 block text-sm text-[color:var(--accent)] hover:underline"
                    onClick={() => setOnboarding(true)}
                  >
                    重新查看引导
                  </button>
                </>
              )}
            </motion.div>
          </div>
        </section>
      </main>

      {/* 选麦弹窗 */}
      <AnimatePresence>
        {micPickerOpen && (
          <motion.div
            className="fixed inset-0 z-50 flex items-center justify-center p-6"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.12 }}
          >
            <button
              className="absolute inset-0 bg-black/40 backdrop-blur-[1px]"
              aria-label="关闭"
              onClick={closeMicPicker}
            />
            <motion.div
              className="elev-pop relative flex max-h-[70vh] w-full max-w-sm flex-col overflow-hidden rounded-2xl border border-[color:var(--border)] bg-[var(--surface)] text-[color:var(--fg)]"
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 10 }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            >
              <div className="flex items-center justify-between px-4 py-3">
                <span className="text-sm font-semibold">选择麦克风</span>
                <button className="btn-del" onClick={closeMicPicker} aria-label="关闭">
                  <X size={15} />
                </button>
              </div>
              <p className="px-4 pb-2 text-xs text-[color:var(--fg-3)]">
                说话时看电平条——没反应就换一个试试。
              </p>
              <div className="flex-1 overflow-y-auto px-2 pb-2">
                {micOptions.map((opt) => {
                  const sel = selectedMic === opt.v;
                  return (
                    <button
                      key={opt.v}
                      className={cn(
                        "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition",
                        sel ? "bg-[var(--accent)] text-white" : "hover:bg-[var(--surface-hover)]",
                      )}
                      onClick={() => chooseMic(opt.v)}
                    >
                      <Mic size={15} className={cn("shrink-0", sel ? "text-white" : "text-[color:var(--fg-3)]")} />
                      <span className="min-w-0 flex-1 truncate text-sm">{opt.n}</span>
                      {sel && (
                        <span
                          className="flex h-5 shrink-0 items-center gap-[1.5px]"
                          style={{ filter: "drop-shadow(0 0 4px rgba(255,255,255,0.45))" }}
                        >
                          {micBars.map((v, i) => (
                            <span
                              key={i}
                              className="w-[2.5px] rounded-full bg-white transition-[height,opacity] duration-150 ease-out"
                              style={{ height: `${2 + v * 16}px`, opacity: 0.5 + 0.5 * v }}
                            />
                          ))}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* 发现新版本弹窗：启动自动提醒 + 关于页手动检查 + dev 预览 */}
      <UpdateModal
        open={updateOpen}
        state={modalState}
        onUpdate={updater.downloadAndInstall}
        onLater={laterUpdate}
        onSkip={skipUpdate}
      />
    </>
  );
}
