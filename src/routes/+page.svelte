<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
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
  } from "lucide-svelte";
  import { fade, fly } from "svelte/transition";

  let recording = $state(false);
  let error = $state(""); // 识别出错
  let injectError = $state(""); // 注入失败（已复制到剪贴板）
  let polishWarn = $state(""); // AI 整理失败时的温和提示

  // 两个独立热键：长按键 / 免按键（空串=未绑定）；可各自录制改键、清除
  let holdKey = $state("");
  let toggleKey = $state("");
  let recordingWhich = $state<"" | "hold" | "toggle">("");
  let pendingMod = $state(""); // 录制时按下的候选单修饰键(e.code)，松开即确认
  let hotkeyError = $state("");
  let polishStyle = $state("default"); // 润色风格：default/bullets/email/raw
  let mics = $state<string[]>([]); // 可选麦克风设备
  let selectedMic = $state(""); // 选定麦克风（空=系统默认）
  // 选麦弹窗 + 实时频谱（与录音胶囊同款：后端 viz.rs 做 FFT 出 N_BANDS 段、前端镜像成中心对称均衡器条）
  let micPickerOpen = $state(false);
  // 频谱条：中心=最低频段(人声基频/能量最足→最高)，越往两边频率越高(齿音时两端窜)。
  const N_BANDS = 11; // 必须与 src-tauri/src/viz.rs 的 N_BANDS 一致
  const WAVE_BARS = N_BANDS * 2 - 1; // 21：镜像后条数
  let micBands = $state<number[]>(Array(N_BANDS).fill(0)); // 平滑后各频段能量 0..1
  const micBars = $derived(
    Array.from({ length: WAVE_BARS }, (_, i) => micBands[Math.abs(i - (WAVE_BARS - 1) / 2)] ?? 0),
  );
  const micOptions = $derived([{ v: "", n: "默认设备" }, ...mics.map((m) => ({ v: m, n: m }))]);

  // ASR 引擎：local(本地 SenseVoice) | cloud(云端)；云端厂商 volc(火山) | ali(阿里)
  let asrEngine = $state("local");
  let cloudVendor = $state("volc");
  let volcApiKey = $state("");
  let volcResourceId = $state("");
  let aliApiKey = $state("");
  let aliModel = $state("qwen3");
  let asrMsg = $state("");
  let asrMsgWarn = $state(false); // asrMsg 是否为警告/失败 → 横幅变琥珀+警告图标，而非绿色
  // 已保存（实际生效）的引擎状态——横幅与「未保存」提示据此判断，跟正在编辑的选择分开
  let savedEngine = $state("local");
  let savedVendor = $state("volc");
  let savedVolcKey = $state("");
  let savedAliKey = $state("");
  // 横幅显示「实际在用」的引擎（已保存态）。云端只有填了 Key 保存后才会是 cloud，故无需「已回退」文案
  const currentEngineLabel = $derived(
    savedEngine === "cloud"
      ? savedVendor === "ali"
        ? "云端 · 阿里 Qwen3"
        : "云端 · 火山豆包"
      : "本地 SenseVoice",
  );
  // 正在编辑的选择与已保存态不一致（如选了云端还没填 Key 保存）→ 提示未保存、仍按已保存态运行
  const asrUnsaved = $derived(
    asrEngine !== savedEngine ||
      (asrEngine === "cloud" &&
        (cloudVendor !== savedVendor ||
          (cloudVendor === "ali" ? aliApiKey !== savedAliKey : volcApiKey !== savedVolcKey))),
  );

  // 主窗口现为 macOS 偏好式布局：左侧导航 + 右侧分组卡片。
  // onboarding 为首启全窗引导（未引导时覆盖显示），引导完进入侧栏。
  let onboarding = $state(false);
  let section = $state<"voice" | "engine" | "ai" | "dict" | "about">("voice");
  let accessibilityOk = $state(false);
  // 本次启动时的辅助功能权限。若启动时没有、运行中才授权，需重启 app 让 CGEventTap 生效。
  let accessibilityAtLaunch = $state<boolean | null>(null);
  const accessibilityNeedsRestart = $derived(accessibilityAtLaunch === false && accessibilityOk);

  const NAV = [
    { id: "voice", label: "语音输入", icon: Mic },
    { id: "engine", label: "识别引擎", icon: AudioLines },
    { id: "ai", label: "AI 整理", icon: Sparkles },
    { id: "dict", label: "词典", icon: BookText },
    { id: "about", label: "关于", icon: Info },
  ] as const;

  const SECTION_TITLE: Record<string, string> = {
    voice: "语音输入",
    engine: "识别引擎",
    ai: "AI 整理",
    dict: "词典",
    about: "关于",
  };

  // 两个独立热键行：长按 / 免按，各自绑定、各自录制改键、各自清除
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

  let providerId = $state("zhipu");
  let baseUrl = $state("");
  let apiKey = $state("");
  let model = $state("");
  let disableThinking = $state(true);
  let saveMsg = $state("");
  let saveOk = $state(false);
  // 每家供应商各自的 Key（base_url → key）：切换带回、保存只更新该家、随 llm.json 持久化
  let keysMap = $state<Record<string, string>>({});

  const currentProvider = $derived(PROVIDERS.find((p) => p.id === providerId));

  function selectProvider(id: string) {
    providerId = id;
    const p = PROVIDERS.find((x) => x.id === id);
    if (p && p.id !== "custom") {
      baseUrl = p.baseUrl;
      model = p.models[0] ?? "";
      disableThinking = p.disableThinking;
      apiKey = keysMap[p.baseUrl] ?? "";
    }
  }

  async function loadSettings() {
    const s = await invoke<{ base_url: string; api_key: string; model: string; disable_thinking: boolean; keys: Record<string, string> }>(
      "get_llm_settings",
    );
    if (s.base_url) {
      baseUrl = s.base_url;
      apiKey = s.api_key;
      model = s.model;
      disableThinking = s.disable_thinking;
      keysMap = s.keys ?? {};
      keysMap[s.base_url] = s.api_key;
      const match = PROVIDERS.find((p) => p.baseUrl === s.base_url);
      providerId = match ? match.id : "custom";
    } else {
      // 没存过生效供应商，但 keys 映射里可能有（如从 ASR 同步过来的通义 Key）——也带上，切过去能回填
      keysMap = s.keys ?? {};
      selectProvider("zhipu");
    }
  }

  async function saveSettings() {
    if (!apiKey || !baseUrl || !model) {
      saveOk = false;
      saveMsg = "请先填好 API Key、Base URL 和模型";
      return;
    }
    saveOk = false;
    saveMsg = "保存中…";
    try {
      keysMap[baseUrl] = apiKey;
      await invoke("set_llm_settings", {
        cfg: { base_url: baseUrl, api_key: apiKey, model, disable_thinking: disableThinking, keys: keysMap },
      });
      saveOk = true;
      saveMsg = "已保存，下次说话即生效";
      // 阿里 Key 通用：通义 Key 同步到 ASR 阿里栏显示（后端已持久化）
      if (baseUrl === "https://dashscope.aliyuncs.com/compatible-mode/v1") aliApiKey = apiKey;
      setTimeout(() => { saveMsg = ""; saveOk = false; }, 2500);
    } catch (e) {
      saveOk = false;
      saveMsg = "保存失败：" + e;
    }
  }

  async function refreshAccessibility() {
    const ok = await invoke<boolean>("check_accessibility");
    if (accessibilityAtLaunch === null) accessibilityAtLaunch = ok;
    accessibilityOk = ok;
  }
  async function grantAccessibility() {
    // 先清掉可能失配的旧授权记录：ad-hoc 签名每次构建 cdhash 会变，旧记录会让系统设置
    // 显示「已勾选」却实际无效；清掉后再弹授权框，让用户干净地授权当前版本。
    await invoke("reset_accessibility_tcc");
    await invoke("request_accessibility");
    setTimeout(refreshAccessibility, 800);
  }
  // 辅助功能授权后 CGEventTap 需重启进程才生效，提供一键重启。
  async function restartApp() {
    await invoke("restart_app");
  }
  // 辅助功能状态：定时 + 窗口重新聚焦时刷新，用户去系统设置改完切回来即时反映。
  onMount(() => {
    const t = setInterval(refreshAccessibility, 2000);
    const onFocus = () => refreshAccessibility();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", onFocus);
    };
  });
  async function finishOnboarding(target: "voice" | "ai" = "voice") {
    await invoke("complete_onboarding");
    onboarding = false;
    section = target;
  }

  onMount(() => {
    invoke<boolean>("get_recording").then((rec) => {
      recording = rec;
    });

    invoke<boolean>("get_onboarded").then((done) => {
      if (!done) onboarding = true;
    });

    invoke<[string, string]>("get_shortcuts").then(([h, t]) => {
      holdKey = h;
      toggleKey = t;
    });

    invoke<string>("get_polish_style").then((s) => {
      if (s) polishStyle = s;
    });

    invoke<string[]>("list_microphones").then((list) => {
      mics = list;
    });
    invoke<string>("get_microphone").then((m) => {
      selectedMic = m;
    });

    // 各分页的数据都先备好，切过去即用
    loadSettings();
    loadVocab();
    loadAsrConfig();
    refreshAccessibility();

    const subs = [
      listen<string>("recording-state", (e) => {
        recording = e.payload === "recording";
        if (recording) {
          // 新一轮录音开始，清掉上一轮的错误提示
          error = "";
          injectError = "";
          polishWarn = "";
        }
      }),
      listen<string>("asr-error", (e) => {
        error = e.payload;
      }),
      listen<string>("inject-error", (e) => {
        injectError = e.payload;
      }),
      listen("llm-error", () => {
        polishWarn = "AI 整理失败，已输出原始识别稿";
      }),
      listen<number[]>("mic-level", (e) => {
        // 后端发来 N_BANDS 段 0..1 能量；每段「快起慢落」EMA——与胶囊一致，跟手又不抖。
        const incoming = e.payload;
        micBands = micBands.map((cur, i) => {
          const t = incoming[i] ?? 0;
          return cur + (t - cur) * (t > cur ? 0.6 : 0.28); // 快起(0.6) / 慢落(0.28)
        });
      }),
    ];
    return () => subs.forEach((s) => s.then((f) => f()));
  });

  // ---- 全局热键：每个模式独立录制 / 清除（单键、单修饰键区分左右、组合键）----
  const SIDE_SYMBOL: Record<string, string> = {
    MetaLeft: "左⌘", MetaRight: "右⌘",
    ControlLeft: "左⌃", ControlRight: "右⌃",
    AltLeft: "左⌥", AltRight: "右⌥",
    ShiftLeft: "左⇧", ShiftRight: "右⇧",
  };
  const COMBO_MOD: Record<string, string> = {
    meta: "⌘", command: "⌘", cmd: "⌘", super: "⌘",
    control: "⌃", ctrl: "⌃", alt: "⌥", option: "⌥", shift: "⇧",
  };
  function tokenPretty(t: string): string {
    if (SIDE_SYMBOL[t]) return SIDE_SYMBOL[t];
    const lc = t.toLowerCase();
    if (COMBO_MOD[lc]) return COMBO_MOD[lc];
    if (/^Key[A-Z]$/.test(t)) return t.slice(3);
    if (/^Digit[0-9]$/.test(t)) return t.slice(5);
    return t; // F1–F20 / Space 等原样
  }
  // 加速器字符串 → 好看的符号（如 "AltRight"→"右⌥"，"Alt+Space"→"⌥Space"）
  function prettyShortcut(accel: string): string {
    if (!accel) return "未设置";
    return accel.split("+").map(tokenPretty).join("");
  }
  // 浏览器 KeyboardEvent.code → 后端认识的主键 token（原样 e.code，仅放行确定能用的）
  function mainKeyToken(code: string): string | null {
    if (/^Key[A-Z]$/.test(code)) return code;
    if (/^Digit[0-9]$/.test(code)) return code;
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
    if (code === "Space") return code;
    return null;
  }
  const MOD_KEYS = ["Meta", "Control", "Alt", "Shift"];
  function startRecord(which: "hold" | "toggle") {
    if (recordingWhich === which) {
      recordingWhich = ""; // 再点一次 = 取消
      pendingMod = "";
      return;
    }
    hotkeyError = "";
    pendingMod = "";
    recordingWhich = which;
  }
  async function setShortcut(which: "hold" | "toggle", accel: string) {
    try {
      await invoke("set_shortcut", { which, accelerator: accel });
      if (which === "hold") holdKey = accel;
      else toggleKey = accel;
      hotkeyError = "";
    } catch (e) {
      hotkeyError = String(e); // 后端拒绝（非法键）：旧键仍有效
    }
  }
  function clearShortcut(which: "hold" | "toggle") {
    setShortcut(which, "");
  }
  function finishRecord(accel: string) {
    const which = recordingWhich;
    recordingWhich = "";
    pendingMod = "";
    if (which) setShortcut(which, accel);
  }
  function onHotkeyKeydown(e: KeyboardEvent) {
    if (micPickerOpen) {
      if (e.key === "Escape") closeMicPicker();
      return;
    }
    if (!recordingWhich) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      recordingWhich = "";
      pendingMod = "";
      return;
    }
    // 按下修饰键：先记为「单修饰键候选」，等松开确认（若随后按主键则升级为组合键）
    if (MOD_KEYS.includes(e.key)) {
      pendingMod = e.code; // 区分左右，如 AltRight
      hotkeyError = "";
      return;
    }
    const key = mainKeyToken(e.code);
    if (!key) {
      hotkeyError = "不支持该按键；可用单个修饰键 / 功能键，或「修饰键 + 字母/数字」组合";
      return;
    }
    const mods: string[] = [];
    if (e.metaKey) mods.push("Meta");
    if (e.ctrlKey) mods.push("Control");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    const isFn = /^F([1-9]|1[0-9]|2[0-4])$/.test(key);
    if (mods.length === 0 && !isFn) {
      hotkeyError = "单个字母 / 数字需配修饰键；或直接用单个修饰键、功能键";
      return;
    }
    finishRecord(mods.length ? [...mods, key].join("+") : key);
  }
  function onHotkeyKeyup(e: KeyboardEvent) {
    if (!recordingWhich || !pendingMod) return;
    // 松开的正是刚按下的候选修饰键、且期间没按主键 → 绑定为单修饰键
    if (e.code === pendingMod) {
      e.preventDefault();
      finishRecord(pendingMod);
    }
  }

  async function setPolishStyle(s: string) {
    polishStyle = s;
    try {
      await invoke("set_polish_style", { style: s });
    } catch (e) {
      console.error(e);
    }
  }

  async function setMic(name: string) {
    selectedMic = name;
    try {
      await invoke("set_microphone", { name });
    } catch (e) {
      console.error(e);
    }
  }

  // ---- 应用内检查更新（查 GitHub 最新 release，只提醒 + 跳转下载，不自动替换）----
  type UpdateInfo = { has_update: boolean; current: string; latest: string; url: string };
  let updateInfo = $state<UpdateInfo | null>(null);
  let checkingUpdate = $state(false);
  let checkedUpdate = $state(false);
  async function checkUpdate() {
    checkingUpdate = true;
    try {
      updateInfo = await invoke<UpdateInfo>("check_update");
      checkedUpdate = true;
    } catch {
      // 静默：网络 / 无 release 不打扰
    } finally {
      checkingUpdate = false;
    }
  }
  onMount(() => {
    checkUpdate(); // 启动后台静默查一次最新版
  });

  // ---- 选麦弹窗：用实时电平条当场测每个麦能不能用 ----
  function startMicMonitor(device: string) {
    invoke("start_mic_monitor", { device }).catch(() => {});
  }
  async function openMicPicker() {
    micPickerOpen = true;
    micBands = Array(N_BANDS).fill(0);
    try {
      mics = await invoke<string[]>("list_microphones"); // 顺带刷新，刚插上的也能看到
    } catch {}
    startMicMonitor(selectedMic);
  }
  function closeMicPicker() {
    micPickerOpen = false;
    micBands = Array(N_BANDS).fill(0);
    invoke("stop_mic_monitor").catch(() => {});
  }
  async function chooseMic(name: string) {
    await setMic(name); // 设为选定并持久化
    micBands = Array(N_BANDS).fill(0);
    startMicMonitor(name); // 监听切到新设备
  }

  // ---- ASR 引擎：本地 SenseVoice / 云端(火山豆包 / 阿里 Qwen3, BYOK) ----
  async function loadAsrConfig() {
    try {
      const c = await invoke<{
        engine: string;
        cloud_vendor: string;
        volc_api_key: string;
        volc_resource_id: string;
        ali_api_key: string;
        ali_model: string;
      }>("get_asr_config");
      cloudVendor = c.cloud_vendor || "volc";
      volcApiKey = c.volc_api_key || "";
      volcResourceId = c.volc_resource_id || "";
      aliApiKey = c.ali_api_key || "";
      aliModel = c.ali_model || "qwen3";
      // 按「实际在用」显示：存的是云端但对应厂商没 Key → 实际跑本地，radio 就显示本地
      //（厂商/Key 仍留在 state，重新选云端时带回；脏 pref 会在下次显式保存时自愈）
      const cloudUsable = cloudVendor === "ali" ? !!aliApiKey.trim() : !!volcApiKey.trim();
      asrEngine = c.engine === "cloud" && cloudUsable ? "cloud" : "local";
      savedEngine = asrEngine;
      savedVendor = cloudVendor;
      savedVolcKey = volcApiKey;
      savedAliKey = aliApiKey;
    } catch (e) {
      console.error(e);
    }
  }
  async function saveAsrConfig() {
    asrMsgWarn = false;
    // 云端必须有对应厂商的 Key 才保存——没 Key 的云端=空选择，会静默回退本地，不让它存
    if (asrEngine === "cloud") {
      const key = (cloudVendor === "ali" ? aliApiKey : volcApiKey).trim();
      if (!key) {
        asrMsg = `请先填入${cloudVendor === "ali" ? "阿里" : "火山"}的 API Key（否则仍用本地 SenseVoice）`;
        asrMsgWarn = true;
        return;
      }
    }
    asrMsg = "保存中…";
    try {
      await invoke("set_asr_config", {
        cfg: {
          engine: asrEngine,
          cloud_vendor: cloudVendor,
          volc_api_key: volcApiKey,
          volc_resource_id: volcResourceId,
          ali_api_key: aliApiKey,
          ali_model: aliModel,
        },
      });
      // 阿里 Key 通用：同步到润色「通义」栏显示（后端 sync_ali_key 已把它写进 llm.json 持久化）
      if (aliApiKey) keysMap["https://dashscope.aliyuncs.com/compatible-mode/v1"] = aliApiKey;
      // 记下已保存态：横幅 / 未保存提示据此判断
      savedEngine = asrEngine;
      savedVendor = cloudVendor;
      savedVolcKey = volcApiKey;
      savedAliKey = aliApiKey;
      asrMsg = "已保存，下次说话生效";
      setTimeout(() => { if (asrMsg === "已保存，下次说话生效") asrMsg = ""; }, 2500);
    } catch (e) {
      asrMsg = "保存失败：" + e;
      asrMsgWarn = true;
    }
  }
  function setAsrEngine(engine: string) {
    asrEngine = engine;
    asrMsg = "";
    // 本地无需配置，点了即时生效；云端只展开配置，要填 Key 点保存才生效
    if (engine === "local") saveAsrConfig();
  }
  function setCloudVendor(v: string) {
    cloudVendor = v;
    asrMsg = "";
  }
  // 把正在编辑的选择回退到「已保存/实际在用」的状态——丢弃未保存的临时选择
  function resetAsrEditing() {
    asrEngine = savedEngine;
    cloudVendor = savedVendor;
    volcApiKey = savedVolcKey;
    aliApiKey = savedAliKey;
    asrMsg = "";
  }
  // 导航：进入识别引擎页前先回到已保存态（没保存的云端选择不残留，回来即默认项）
  function selectSection(id: "voice" | "engine" | "ai" | "dict" | "about") {
    if (id === "engine") resetAsrEditing();
    section = id;
  }

  // ---- 词典：拼音纠错词 / 软术语 / 硬替换；增删即时保存，下次说话即生效 ----
  type Replacement = { wrong: string; right: string };
  type VocabData = {
    soft_terms: string[];
    pinyin_terms: string[];
    replacements: Replacement[];
  };
  let vocab = $state<VocabData>({ soft_terms: [], pinyin_terms: [], replacements: [] });
  let vocabMsg = $state("");
  let newPinyin = $state("");
  let newSoft = $state("");
  let newWrong = $state("");
  let newRight = $state("");

  async function loadVocab() {
    vocab = await invoke<VocabData>("get_vocab");
  }
  async function saveVocab() {
    vocabMsg = "保存中…";
    try {
      await invoke("set_vocab", { data: $state.snapshot(vocab) });
      vocabMsg = "已保存，下次说话即生效";
    } catch (e) {
      vocabMsg = "保存失败：" + e;
    }
  }
  function addPinyin() {
    const t = newPinyin.trim();
    newPinyin = "";
    if (!t || vocab.pinyin_terms.includes(t)) return;
    vocab.pinyin_terms.push(t);
    saveVocab();
  }
  function removePinyin(i: number) {
    vocab.pinyin_terms.splice(i, 1);
    saveVocab();
  }
  function addSoft() {
    const t = newSoft.trim();
    newSoft = "";
    if (!t || vocab.soft_terms.includes(t)) return;
    vocab.soft_terms.push(t);
    saveVocab();
  }
  function removeSoft(i: number) {
    vocab.soft_terms.splice(i, 1);
    saveVocab();
  }
  function addReplacement() {
    const wrong = newWrong.trim();
    const right = newRight.trim();
    if (!wrong || !right) return;
    newWrong = "";
    newRight = "";
    const idx = vocab.replacements.findIndex((r) => r.wrong === wrong);
    if (idx >= 0) vocab.replacements[idx].right = right; // 同一误识覆盖为新写法
    else vocab.replacements.push({ wrong, right });
    saveVocab();
  }
  function removeReplacement(i: number) {
    vocab.replacements.splice(i, 1);
    saveVocab();
  }

  // ---- 自学建议（步骤③）：常被 AI 纠正的同音词，浮现供一键加入 / 忽略 ----
  type LearnedPair = { wrong: string; right: string; count: number };
  let suggestions = $state<LearnedPair[]>([]);
  async function loadSuggestions() {
    try {
      suggestions = await invoke<LearnedPair[]>("get_learned_suggestions");
    } catch (e) {
      console.error(e);
    }
  }
  async function acceptSuggestion(s: LearnedPair) {
    try {
      await invoke("accept_learned_suggestion", { right: s.right });
      await loadVocab();
      await loadSuggestions();
      vocabMsg = `已把「${s.right}」加为拼音纠错词`;
    } catch (e) {
      vocabMsg = "操作失败：" + e;
    }
  }
  async function dismissSuggestion(s: LearnedPair) {
    try {
      await invoke("dismiss_learned_suggestion", { wrong: s.wrong, right: s.right });
      await loadSuggestions();
    } catch (e) {
      console.error(e);
    }
  }
  // 进入「词典」页时刷新建议（每次出稿后可能新增）
  $effect(() => {
    if (section === "dict") loadSuggestions();
  });

  // ---- 共用样式：统一的卡片 / 行 / 文字层次，保持一致的呼吸节奏 ----
  // 视觉收敛到 app.css 的 token 驱动组件类（.card/.ui-row/.input… 一处定义、浅深色自适应），
  // 这里只留语义别名；附加的布局 utility（mt-4 / p-4 等）仍可按需拼接。
  const cardCls = "card";
  // 分组小标题：小号克制的灰，把「这是哪一组」压成次要信息，让正文更突出
  const cardHeadCls = "card-head";
  const dividerCls = "divide-y divide-[color:var(--hairline)]";
  const rowCls = "ui-row";
  const descCls = "desc";
  // section 顶部的一句话说明：统一间距与字号，每页节奏一致
  const leadCls = "lead";
  const inputCls = "input";
  const addBtnCls = "btn-add";
  const delBtnCls = "btn-del";
</script>

{#snippet radio(sel: boolean)}
  <span
    class={"grid h-[18px] w-[18px] shrink-0 place-items-center rounded-full transition " +
      (sel ? "bg-[var(--accent)]" : "border border-[color:var(--control-border)]")}
  >
    {#if sel}<span class="h-1.5 w-1.5 rounded-full bg-white"></span>{/if}
  </span>
{/snippet}

<svelte:window onkeydown={onHotkeyKeydown} onkeyup={onHotkeyKeyup} />

{#if onboarding}
  <!-- 首次引导：全窗居中 -->
  <main
    class="flex h-screen flex-col bg-[var(--bg)] text-[color:var(--fg)]"
  >
    <div data-tauri-drag-region class="h-10 shrink-0"></div>
    <div class="flex flex-1 items-center justify-center overflow-y-auto px-6 pb-8">
      <div class="w-full max-w-md space-y-5">
        <div>
          <h2 class="flex items-center gap-2 text-lg font-semibold">
            <Mic size={20} class="text-[color:var(--accent)]" /> 欢迎使用 Untype<span class="text-[color:var(--accent)]">.</span>
          </h2>
          <p class="mt-1 text-sm leading-relaxed text-[color:var(--fg-2)]">
            按 <kbd class="rounded border border-[color:var(--hairline)] bg-[var(--kbd-bg)] px-1.5 py-0.5 font-mono">{prettyShortcut(holdKey || toggleKey)}</kbd>
            说话，松开后自动转成文字、AI 整理，并注入到光标处。
          </p>
        </div>

        <div class={cardCls + " p-4"}>
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium">① 辅助功能权限（推荐）</span>
            {#if accessibilityOk}
              <span class="inline-flex items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
                <Check size={14} /> 已授权
              </span>
            {:else}
              <button
                class="rounded-md bg-[var(--accent)] px-3 py-1 text-xs font-medium text-white transition hover:bg-[var(--accent-hover)]"
                onclick={grantAccessibility}
              >
                开启
              </button>
            {/if}
          </div>
          <p class="mt-1 text-xs leading-relaxed text-[color:var(--fg-2)]">
            用于把文字自动注入到光标。不授也能用——会自动复制到剪贴板，⌘V 粘贴。
          </p>
        </div>

        <div class={cardCls + " p-4"}>
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium">② AI 整理（可选）</span>
            <button
              class="rounded-md border border-[color:var(--control-border)] bg-[var(--control)] px-3 py-1 text-xs font-medium transition hover:bg-[var(--control-hover)]"
              onclick={() => finishOnboarding("ai")}
            >
              去配置
            </button>
          </div>
          <p class="mt-1 text-xs leading-relaxed text-[color:var(--fg-2)]">
            配个云模型（有免费档），自动去口水词、分点、纠正同音字。不配也能用，只是输出原始识别文本。
          </p>
        </div>

        <p class="flex items-start gap-1.5 text-xs text-[color:var(--fg-3)]">
          <Info size={13} class="mt-0.5 shrink-0" />
          <span>麦克风权限会在你第一次说话时由系统自动弹窗请求，点「允许」即可。</span>
        </p>

        <button
          class="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-[var(--accent)] py-2.5 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
          onclick={() => finishOnboarding("voice")}
        >
          开始使用 <ArrowRight size={15} />
        </button>
      </div>
    </div>
  </main>
{:else}
  <!-- 主界面：左侧导航 + 右侧内容 -->
  <main
    class="flex h-screen bg-[var(--bg)] text-[color:var(--fg)]"
  >
    <!-- 侧栏 -->
    <aside class="flex w-[200px] shrink-0 flex-col border-r border-[color:var(--border)] px-3">
      <div data-tauri-drag-region class="h-10 shrink-0"></div>
      <nav class="flex flex-col gap-1">
        {#each NAV as item}
          {@const Icon = item.icon}
          <button
            class={"flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-sm font-medium transition " +
              (section === item.id
                ? "bg-[var(--accent)] text-white shadow-sm"
                : "text-[color:var(--fg-2)] hover:bg-[var(--surface-hover)] hover:text-[color:var(--fg)]")}
            onclick={() => selectSection(item.id)}
          >
            <Icon size={16} />
            {item.label}
          </button>
        {/each}
      </nav>
    </aside>

    <!-- 内容 -->
    <section class="flex flex-1 flex-col overflow-hidden">
      <div data-tauri-drag-region class="h-10 shrink-0"></div>
      <!-- 固定标题：常驻 header，不随内容滚动 -->
      <div class="flex shrink-0 items-center justify-between gap-3 px-7 pb-5">
        <h1 class="text-[17px] font-semibold tracking-tight">{SECTION_TITLE[section]}</h1>
        {#if section === "voice"}
          <span class="status-pill">
            <span class="led" class:on={recording} class:animate-pulse={recording}></span>
            {recording ? "录音中…" : "空闲"}
          </span>
        {/if}
      </div>
      <div class="flex-1 overflow-y-auto px-7 pb-12">
        {#key section}
        <div in:fly={{ y: 6, duration: 160 }}>
        {#if section === "voice"}
          <!-- 快捷键：长按 / 免按各一个独立可改键 + ✕ 清除 -->
          <div class={cardCls}>
            <div class={cardHeadCls}>快捷键</div>
            {#if !accessibilityOk}
              <div class="mx-4 mb-1 flex items-center gap-2.5 rounded-md bg-amber-50 px-2.5 py-1.5 text-xs leading-snug text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
                <span class="min-w-0 flex-1">单键热键需要「辅助功能」权限，点「开启」去系统设置授权。</span>
                <button
                  class="shrink-0 rounded bg-amber-600/90 px-2.5 py-1 font-medium text-white transition hover:bg-amber-600 dark:bg-amber-500/90 dark:hover:bg-amber-500"
                  onclick={grantAccessibility}
                >
                  开启
                </button>
              </div>
            {:else if accessibilityNeedsRestart}
              <div class="mx-4 mb-1 flex items-center gap-2.5 rounded-md bg-emerald-50 px-2.5 py-1.5 text-xs leading-snug text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300">
                <span class="min-w-0 flex-1">✓ 辅助功能已授权，重启 app 后单键热键即生效。</span>
                <button
                  class="shrink-0 rounded bg-emerald-600 px-2.5 py-1 font-medium text-white transition hover:bg-emerald-700 dark:bg-emerald-600 dark:hover:bg-emerald-500"
                  onclick={restartApp}
                >
                  立即重启
                </button>
              </div>
            {/if}
            <div class={dividerCls}>
              {#each KEY_ROWS as row}
                {@const key = row.which === "hold" ? holdKey : toggleKey}
                {@const rec = recordingWhich === row.which}
                <div class={rowCls}>
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">{row.title}</div>
                    <div class={descCls}>{row.desc}</div>
                  </div>
                  <div class="flex shrink-0 items-center gap-1.5">
                    <button class="kbd" class:recording={rec} onclick={() => startRecord(row.which)}>
                      {rec ? "按住要绑的键…" : prettyShortcut(key)}
                    </button>
                    <button
                      class={delBtnCls}
                      title={rec ? "取消" : "清除"}
                      aria-label="清除该热键"
                      disabled={!key && !rec}
                      onclick={() => (rec ? (recordingWhich = "") : clearShortcut(row.which))}
                    >
                      <X size={14} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
            {#if hotkeyError}
              <p class="px-4 pt-2 text-xs text-red-500">{hotkeyError}</p>
            {/if}
            <p class="px-4 pt-1 pb-3 text-xs leading-relaxed text-[color:var(--fg-3)]">
              点徽章后按要绑的键：可单个修饰键（如 右⌥）、功能键，或「修饰键 + 字母/数字」组合；免按为双击触发。两个模式可同时启用，✕ 清除即停用，Esc 取消。
            </p>
          </div>

          <!-- 麦克风 -->
          <div class={cardCls + " mt-4"}>
            <button
              class={rowCls + " justify-between tap"}
              onclick={openMicPicker}
            >
              <div class="text-sm font-medium">麦克风</div>
              <span class="flex min-w-0 items-center gap-1.5 text-sm text-[color:var(--fg-2)]">
                <span class="truncate">{selectedMic || "默认设备"}</span>
                <ChevronDown size={14} class="shrink-0 text-[color:var(--fg-3)]" />
              </span>
            </button>
          </div>

          <!-- 整理风格 -->
          <div class={cardCls + " mt-4"}>
            <div class={cardHeadCls}>整理风格</div>
            <div class={dividerCls}>
              {#each STYLES as s}
                <button
                  class={rowCls + " tap"}
                  onclick={() => setPolishStyle(s.id)}
                >
                  {@render radio(polishStyle === s.id)}
                  <div class="min-w-0 flex-1">
                    <div class="text-sm font-medium">{s.title}</div>
                    <div class={descCls}>{s.desc}</div>
                  </div>
                </button>
              {/each}
            </div>
          </div>

          <!-- 仅在出错 / 需注意时提示；正常听写靠底部胶囊反馈，不显示字幕 -->
          {#if error || injectError || polishWarn}
            <div class={cardCls + " mt-4"}>
              <div class="space-y-2 px-4 py-3">
                {#if error}
                  <p class="text-sm text-red-500">识别出错：{error}</p>
                {/if}
                {#if polishWarn}
                  <p class="text-xs text-amber-600 dark:text-amber-400">{polishWarn}</p>
                {/if}
                {#if injectError}
                  <p class="flex items-start gap-1.5 text-xs text-amber-600 dark:text-amber-400">
                    <TriangleAlert size={14} class="mt-0.5 shrink-0" />
                    <span>
                      已识别但无法自动注入——文本已复制到剪贴板，按
                      <kbd class="rounded bg-amber-100 px-1 dark:bg-amber-900">⌘V</kbd>
                      粘贴即可。如需自动注入，请到「系统设置 → 隐私与安全性 → 辅助功能」给本 app 授权。
                    </span>
                  </p>
                {/if}
              </div>
            </div>
          {/if}
        {:else if section === "engine"}
          <div class={"mb-4 flex items-center gap-2 rounded-lg border px-3 py-2 text-sm " +
            (asrMsg && asrMsgWarn
              ? "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-800/60 dark:bg-amber-950/30 dark:text-amber-300"
              : "border-green-300 bg-green-50 text-green-800 dark:border-green-800/60 dark:bg-green-950/30 dark:text-green-300")}>
            {#if asrMsg && asrMsgWarn}
              <TriangleAlert size={15} class="shrink-0" />
            {:else}
              <Check size={15} class="shrink-0" />
            {/if}
            <span>当前使用 <span class="font-semibold">{currentEngineLabel}</span>{asrMsg ? ` · ${asrMsg}` : ""}</span>
          </div>
          <!-- 识别引擎：本地 / 云端 -->
          <div class={cardCls}>
            <div class={cardHeadCls}>识别引擎</div>
            <div class={dividerCls}>
              <button
                class={rowCls + " tap"}
                onclick={() => setAsrEngine("local")}
              >
                {@render radio(asrEngine === "local")}
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">本地 SenseVoice</div>
                  <div class={descCls}>离线 · 免费 · 隐私；自带标点，中英混说 OK</div>
                </div>
              </button>
              <button
                class={rowCls + " tap"}
                onclick={() => setAsrEngine("cloud")}
              >
                {@render radio(asrEngine === "cloud")}
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">云端（火山 / 阿里）</div>
                  <div class={descCls}>中英混说 / 技术词 / 标点更强；音频上云、按量计费</div>
                </div>
              </button>
            </div>
            {#if asrEngine === "cloud"}
              <div class="space-y-3 p-4 pt-3">
                <!-- 厂商子选择 -->
                <div class="seg w-full">
                  {#each [{ id: "volc", name: "火山豆包" }, { id: "ali", name: "阿里 Qwen3" }] as v}
                    <button class:on={cloudVendor === v.id} onclick={() => setCloudVendor(v.id)}>
                      {v.name}
                    </button>
                  {/each}
                </div>

                {#if cloudVendor === "volc"}
                  <label class="block text-sm font-medium">
                    火山 API Key
                    <input
                      class={inputCls + " mt-1.5"}
                      type="password"
                      bind:value={volcApiKey}
                      placeholder="火山控制台 → 语音技术 → API Key"
                    />
                  </label>
                  <p class="flex items-start gap-1.5 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                    <Info size={13} class="mt-0.5 shrink-0" />
                    <span>
                      去
                      <button class="text-[color:var(--accent)] hover:underline" onclick={() => openUrl("https://console.volcengine.com/speech/new/setting/apikeys")}>火山控制台</button>
                      开通「流式语音识别大模型」取 Key（有免费试用额度，以控制台为准）；支持热词偏置（吃你的词典）。
                    </span>
                  </p>
                {:else}
                  <label class="block text-sm font-medium">
                    阿里 DashScope API Key
                    <input
                      class={inputCls + " mt-1.5"}
                      type="password"
                      bind:value={aliApiKey}
                      placeholder="百炼控制台 → API-KEY"
                    />
                  </label>
                  <p class="flex items-start gap-1.5 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                    <Info size={13} class="mt-0.5 shrink-0" />
                    <span>
                      去
                      <button class="text-[color:var(--accent)] hover:underline" onclick={() => openUrl("https://bailian.console.aliyun.com/")}>百炼控制台</button>
                      创建 API-KEY（新用户有免费额度）。用 Qwen3-ASR-Flash（大模型 ASR，中英 / 技术词更强）；暂不带内联热词，靠模型 + 词典纠错。
                    </span>
                  </p>
                {/if}

                <p class="text-xs text-neutral-400 dark:text-neutral-500">音频会上传到对应云端。</p>

                <div class="flex items-center gap-3 pt-1">
                  <button
                    class="rounded-lg bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
                    onclick={saveAsrConfig}
                  >
                    保存
                  </button>
                  {#if asrUnsaved}
                    <span class="text-xs text-amber-600 dark:text-amber-400">未保存 · 当前仍按「{currentEngineLabel}」运行，点保存生效</span>
                  {:else}
                    <span class="text-xs text-neutral-400 dark:text-neutral-500">已保存，下次说话即用此引擎</span>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {:else if section === "ai"}
          <p class={leadCls}>
            选一个云端整理模型（OpenAI 兼容）。标「免费」的可零成本用；Key 只存在本机。
          </p>

          <!-- 供应商 -->
          <div class={cardCls}>
            <div class={cardHeadCls}>供应商</div>
            <div class="grid grid-cols-2 gap-2.5 px-4 pb-4">
              {#each PROVIDERS as p}
                <button
                  class={"flex flex-col items-start gap-1 rounded-lg border p-3 text-left transition " +
                    (providerId === p.id
                      ? "border-[color:var(--accent)] bg-[var(--accent-soft)]"
                      : "border-[color:var(--border)] hover:border-[color:var(--fg-3)]")}
                  onclick={() => selectProvider(p.id)}
                >
                  <span class="flex items-center gap-1.5 text-sm font-medium">
                    {p.name}
                    {#if p.free}
                      <span class="rounded bg-green-100 px-1.5 py-0.5 text-[10px] font-medium text-green-700 dark:bg-green-900/60 dark:text-green-300">免费</span>
                    {/if}
                  </span>
                  <span class="text-xs leading-snug text-neutral-500 dark:text-neutral-400">{p.note}</span>
                </button>
              {/each}
            </div>
            {#if currentProvider?.register}
              <button
                class="inline-flex items-center gap-1 px-4 pb-4 text-sm text-[color:var(--accent)] hover:underline"
                onclick={() => openUrl(currentProvider!.register)}
              >
                去 {currentProvider.name} 获取 API Key <ExternalLink size={13} />
              </button>
            {/if}
          </div>

          <!-- 配置 -->
          <div class={cardCls + " mt-4 space-y-4 p-4"}>
            <label class="block text-sm font-medium">
              API Key
              <input class={inputCls + " mt-1.5"} type="password" bind:value={apiKey} placeholder="粘贴你的 Key" />
            </label>

            <div>
              <p class="mb-1.5 text-sm font-medium">模型</p>
              {#if (currentProvider?.models ?? []).length}
                <div class="mb-2 flex flex-wrap gap-1.5">
                  {#each currentProvider?.models ?? [] as m}
                    <button
                      class={"rounded-full border px-2.5 py-1 text-xs transition " +
                        (model === m
                          ? "border-[color:var(--accent)] bg-[var(--accent-soft)] text-[color:var(--accent)]"
                          : "border-[color:var(--border)] hover:border-[color:var(--fg-3)]")}
                      onclick={() => (model = m)}
                    >
                      {m}
                    </button>
                  {/each}
                </div>
              {/if}
              <input class={inputCls} bind:value={model} placeholder="或手填模型名" />
            </div>

            <label class="block text-sm font-medium">
              Base URL
              <input class={inputCls + " mt-1.5"} bind:value={baseUrl} placeholder="https://.../v1" />
            </label>

            <div class="flex items-center gap-3 pt-1">
              <button
                class="rounded-lg bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
                onclick={saveSettings}
              >
                保存
              </button>
              {#if saveMsg}
                <span class={"text-sm " + (saveOk ? "text-green-600 dark:text-green-400" : "text-amber-600 dark:text-amber-400")}>{saveMsg}</span>
              {/if}
            </div>
          </div>
        {:else if section === "dict"}
          <p class="mb-1 text-sm leading-relaxed text-neutral-500 dark:text-neutral-400">
            教 app 认准你的常用词、纠正同音误识。改完即时生效，下次说话就用上。
          </p>
          <p class="mb-4 h-4 text-xs text-green-600 dark:text-green-400">{vocabMsg}</p>

          <!-- 自学建议：常被 AI 纠正的同音词，一键加入 / 忽略 -->
          {#if suggestions.length}
            <div class={cardCls + " mb-4 border-[color:var(--accent)]"}>
              <div class={cardHeadCls}>建议 · 自动学习</div>
              <p class="px-4 pb-2 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
                这些词常被 AI 纠正。加为拼音纠错词后会直接确定性纠对（一条顶所有同音变体），越用越准。
              </p>
              <div class={dividerCls}>
                {#each suggestions as s (s.wrong + "→" + s.right)}
                  <div class={rowCls + " justify-between gap-2"}>
                    <span class="inline-flex min-w-0 items-center gap-1.5 text-sm">
                      <span class="truncate text-neutral-400 line-through">{s.wrong}</span>
                      <ArrowRight size={13} class="shrink-0 text-neutral-400" />
                      <span class="truncate font-medium">{s.right}</span>
                      <span class="shrink-0 text-xs text-neutral-400">×{s.count}</span>
                    </span>
                    <span class="flex shrink-0 items-center gap-1.5">
                      <button
                        class="rounded-md bg-[var(--accent)] px-2.5 py-1 text-xs font-medium text-white transition hover:bg-[var(--accent-hover)]"
                        onclick={() => acceptSuggestion(s)}
                      >
                        加入
                      </button>
                      <button class={delBtnCls} onclick={() => dismissSuggestion(s)} aria-label="忽略">
                        <X size={14} />
                      </button>
                    </span>
                  </div>
                {/each}
              </div>
            </div>
          {/if}

          <!-- 拼音纠错词（核心：确定性同音归一）-->
          <div class={cardCls}>
            <div class={cardHeadCls}>拼音纠错词</div>
            <p class="px-4 pb-3 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
              读音相同就强制纠成它，一条顶所有同音变体（如「中文强」纠掉「中文墙 / 中文枪」）。适合常被同音听错、自身又不是日常常用词的词。
            </p>
            <div class="flex flex-wrap gap-2 px-4">
              {#each vocab.pinyin_terms as t, i}
                <span
                  class="inline-flex items-center gap-1 rounded-full border border-[color:var(--accent)] bg-[var(--accent-soft)] py-1 pl-3 pr-1.5 text-sm text-[color:var(--accent)]"
                >
                  {t}
                  <button
                    class="grid h-5 w-5 place-items-center rounded-full text-[color:var(--accent)] opacity-70 transition hover:bg-[var(--accent-soft)] hover:opacity-100"
                    onclick={() => removePinyin(i)}
                    aria-label="删除"
                  >
                    <X size={12} />
                  </button>
                </span>
              {/each}
              {#if !vocab.pinyin_terms.length}
                <span class="text-xs text-neutral-400">还没有，下面添加</span>
              {/if}
            </div>
            <div class="flex items-center gap-2 p-4">
              <input
                class={inputCls}
                placeholder="正确写法，如 中文强"
                bind:value={newPinyin}
                onkeydown={(e) => e.key === "Enter" && addPinyin()}
              />
              <button class={addBtnCls} onclick={addPinyin} disabled={!newPinyin.trim()} aria-label="添加">
                <Plus size={16} />
              </button>
            </div>
          </div>

          <!-- 软术语（仅供 LLM 参考，不强制替换）-->
          <div class={cardCls + " mt-4"}>
            <div class={cardHeadCls}>软术语</div>
            <p class="px-4 pb-3 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
              只交给 AI 参考、结合上下文判断，不强制替换。适合「优化 / 流式」这类本身常用、强纠会误伤的词。
            </p>
            <div class="flex flex-wrap gap-2 px-4">
              {#each vocab.soft_terms as t, i}
                <span
                  class="inline-flex items-center gap-1 rounded-full border border-[color:var(--border)] bg-[var(--surface-hover)] py-1 pl-3 pr-1.5 text-sm"
                >
                  {t}
                  <button
                    class="grid h-5 w-5 place-items-center rounded-full text-[color:var(--fg-3)] transition hover:text-[color:var(--fg)]"
                    onclick={() => removeSoft(i)}
                    aria-label="删除"
                  >
                    <X size={12} />
                  </button>
                </span>
              {/each}
              {#if !vocab.soft_terms.length}
                <span class="text-xs text-neutral-400">还没有，下面添加</span>
              {/if}
            </div>
            <div class="flex items-center gap-2 p-4">
              <input
                class={inputCls}
                placeholder="术语正确写法，如 SenseVoice"
                bind:value={newSoft}
                onkeydown={(e) => e.key === "Enter" && addSoft()}
              />
              <button class={addBtnCls} onclick={addSoft} disabled={!newSoft.trim()} aria-label="添加">
                <Plus size={16} />
              </button>
            </div>
          </div>

          <!-- 硬替换（确定性整串替换）-->
          <div class={cardCls + " mt-4"}>
            <div class={cardHeadCls}>硬替换</div>
            <p class="px-4 pb-2 text-xs leading-snug text-neutral-400 dark:text-neutral-500">
              把固定错字整串换成对的（如「停路 → 停录」）。只放几乎总错成同一写法、且不会误伤的。
            </p>
            <div class={dividerCls}>
              {#each vocab.replacements as r, i}
                <div class={rowCls + " justify-between"}>
                  <span class="inline-flex min-w-0 items-center gap-1.5 text-sm">
                    <span class="truncate text-neutral-400 line-through">{r.wrong}</span>
                    <ArrowRight size={13} class="shrink-0 text-neutral-400" />
                    <span class="truncate font-medium">{r.right}</span>
                  </span>
                  <button class={delBtnCls} onclick={() => removeReplacement(i)} aria-label="删除">
                    <X size={14} />
                  </button>
                </div>
              {/each}
              {#if !vocab.replacements.length}
                <div class="px-4 py-3 text-xs text-neutral-400">还没有，下面添加</div>
              {/if}
            </div>
            <div class="flex items-center gap-2 p-4">
              <input
                class={inputCls}
                placeholder="误识"
                bind:value={newWrong}
                onkeydown={(e) => e.key === "Enter" && addReplacement()}
              />
              <ArrowRight size={16} class="shrink-0 text-neutral-400" />
              <input
                class={inputCls}
                placeholder="正确"
                bind:value={newRight}
                onkeydown={(e) => e.key === "Enter" && addReplacement()}
              />
              <button
                class={addBtnCls}
                onclick={addReplacement}
                disabled={!newWrong.trim() || !newRight.trim()}
                aria-label="添加"
              >
                <Plus size={16} />
              </button>
            </div>
          </div>
        {:else}
          <!-- 关于 -->
          <div class="mb-5 flex flex-col items-center gap-2 text-center">
            <!-- Untype LOGO 本体：声波 → 基线 → 文字光标 的连续笔画（沿用 app 图标的笔画）；
                 墨色随主题（var(--fg)，浅/深自适应），唯一的强调蓝只落在末端光标。 -->
            <svg viewBox="200 366 612 292" class="h-12 w-auto" fill="none" aria-hidden="true">
              <path
                d="M 226 512 C 256 384, 300 384, 330 512 C 360 640, 404 640, 432 512 C 456 424, 494 446, 520 512 C 540 558, 562 530, 582 512 L 742 512"
                stroke="var(--fg)"
                stroke-width="40"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <rect x="752" y="404" width="42" height="216" rx="21" fill="var(--accent)" />
            </svg>
            <!-- 字标：Untype + 蓝句点（呼应 OpenDesign 的 Untype. 字样） -->
            <div class="text-2xl font-semibold tracking-tight">Untype<span class="text-[color:var(--accent)]">.</span></div>
            <div class="text-xs text-[color:var(--fg-3)]">语音听写 · v{updateInfo?.current ?? "0.1.0"}</div>
            <!-- 应用内检查更新：克制风，一行文字 / 链接 -->
            <div class="text-xs">
              {#if checkingUpdate}
                <span class="text-[color:var(--fg-3)]">检查更新中…</span>
              {:else if updateInfo?.has_update}
                <button class="text-[color:var(--accent)] hover:underline" onclick={() => openUrl(updateInfo!.url)}>
                  有新版本 v{updateInfo.latest} · 前往下载
                </button>
              {:else if checkedUpdate}
                <span class="text-[color:var(--fg-3)]">已是最新</span>
                <button class="ml-1 text-[color:var(--fg-3)] underline hover:text-[color:var(--fg-2)]" onclick={checkUpdate}>重新检查</button>
              {:else}
                <button class="text-[color:var(--accent)] hover:underline" onclick={checkUpdate}>检查更新</button>
              {/if}
            </div>
            <p class="max-w-xs text-sm leading-relaxed text-[color:var(--fg-2)]">
              说话，不用打字——本地 SenseVoice 识别 + 云端 AI 轻整理，按住热键说话即转成文字、注入光标。
            </p>
          </div>

          <div class={cardCls}>
            <div class={cardHeadCls}>权限</div>
            <div class={dividerCls}>
              <div class={rowCls}>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">辅助功能</div>
                  <div class={descCls}>把文字自动注入光标；未授权则复制到剪贴板，⌘V 粘贴。</div>
                </div>
                {#if accessibilityNeedsRestart}
                  <button
                    class="shrink-0 rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white transition hover:bg-emerald-700"
                    onclick={restartApp}
                  >
                    重启生效
                  </button>
                {:else if accessibilityOk}
                  <span class="inline-flex shrink-0 items-center gap-1 text-xs font-medium text-green-600 dark:text-green-400">
                    <Check size={14} /> 已授权
                  </span>
                {:else}
                  <button
                    class="shrink-0 rounded-md bg-[var(--accent)] px-3 py-1 text-xs font-medium text-white transition hover:bg-[var(--accent-hover)]"
                    onclick={grantAccessibility}
                  >
                    开启
                  </button>
                {/if}
              </div>
              <div class={rowCls}>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium">麦克风</div>
                  <div class={descCls}>首次说话时由系统弹窗申请，点「允许」即可。</div>
                </div>
              </div>
            </div>
          </div>

          <div class={cardCls + " mt-4"}>
            <div class={cardHeadCls}>信息</div>
            <div class={dividerCls}>
              <div class={rowCls}>
                <div class="flex-1 text-sm font-medium">全局热键</div>
                <span class="shrink-0 text-sm text-neutral-500 dark:text-neutral-400">
                  长按 {prettyShortcut(holdKey)} · 免按 {prettyShortcut(toggleKey)}
                </span>
              </div>
              <div class={rowCls}>
                <div class="flex-1 text-sm font-medium">识别引擎</div>
                <span class="shrink-0 text-sm text-neutral-500 dark:text-neutral-400">{currentEngineLabel}</span>
              </div>
              <div class={rowCls}>
                <div class="flex-1 text-sm font-medium">整理模型</div>
                <span class="shrink-0 text-sm text-neutral-500 dark:text-neutral-400">
                  {currentProvider?.name ?? "未配置"}{model ? " · " + model : ""}
                </span>
              </div>
            </div>
          </div>

          <button
            class="mx-auto mt-5 block text-sm text-[color:var(--accent)] hover:underline"
            onclick={() => (onboarding = true)}
          >
            重新查看引导
          </button>
        {/if}
        </div>
        {/key}
      </div>
    </section>
  </main>

  {#if micPickerOpen}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-6" transition:fade={{ duration: 120 }}>
      <button class="absolute inset-0 bg-black/40 backdrop-blur-[1px]" aria-label="关闭" onclick={closeMicPicker}></button>
      <div
        class="elev-pop relative flex max-h-[70vh] w-full max-w-sm flex-col overflow-hidden rounded-2xl border border-[color:var(--border)] bg-[var(--surface)] text-[color:var(--fg)]"
        in:fly={{ y: 10, duration: 180 }}
      >
        <div class="flex items-center justify-between px-4 py-3">
          <span class="text-sm font-semibold">选择麦克风</span>
          <button class={delBtnCls} onclick={closeMicPicker} aria-label="关闭"><X size={15} /></button>
        </div>
        <p class="px-4 pb-2 text-xs text-[color:var(--fg-3)]">
          说话时看电平条——没反应就换一个试试。
        </p>
        <div class="flex-1 overflow-y-auto px-2 pb-2">
          {#each micOptions as opt (opt.v)}
            {@const sel = selectedMic === opt.v}
            <button
              class={"flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition " +
                (sel
                  ? "bg-[var(--accent)] text-white"
                  : "hover:bg-[var(--surface-hover)]")}
              onclick={() => chooseMic(opt.v)}
            >
              <Mic size={15} class={"shrink-0 " + (sel ? "text-white" : "text-[color:var(--fg-3)]")} />
              <span class="min-w-0 flex-1 truncate text-sm">{opt.n}</span>
              {#if sel}
                <span
                  class="flex h-5 shrink-0 items-center gap-[1.5px]"
                  style="filter: drop-shadow(0 0 4px rgba(255,255,255,0.45))"
                >
                  {#each micBars as v, i (i)}
                    <span
                      class="w-[2.5px] rounded-full bg-white transition-[height,opacity] duration-150 ease-out"
                      style="height: {2 + v * 16}px; opacity: {0.5 + 0.5 * v}"
                    ></span>
                  {/each}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </div>
  {/if}
{/if}
