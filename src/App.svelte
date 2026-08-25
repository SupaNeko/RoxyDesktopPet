<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { emit, listen } from '@tauri-apps/api/event';
  import { ListTodo, MessageCircle, Mic, MicOff, MousePointer2, Send, Settings, Trash2, X } from 'lucide-svelte';
  import { getRuntimeStatus, getSettings, listMessages, saveSettings, sendMessage, startVoiceListening, stopVoiceListening, listMicrophoneDevices, testVoiceOutput, startGptSovits, listTodos, deleteTodo, openAppWindow, openPetMenu, toggleProactiveEnabled, setPushToTalkShortcut, beginShortcutCapture, cancelShortcutCapture, setMousePassthrough, getMousePassthrough, listToolHookSupport, writeToolHookConfig, removeToolHookConfig, testToolHook, updatePetHitTestLayout, getTaskbarApps, getHardwareStats, getNowPlaying } from './lib/api';
  import type { AppSettings, Message, RuntimeStatus, Todo, ToolHookToolInfo, VoiceStatus } from './lib/types';
  import type { CapturedBinding } from './lib/api';
  const label = '__TAURI_INTERNALS__' in window ? getCurrentWindow().label : new URLSearchParams(location.search).get('view') ?? 'pet';
  let settings = $state<AppSettings>({ pet_name: 'ChatPet', persona: '', user_name: '你', api_base_url: '', api_model: '', api_key_configured: false, voice_output_enabled: false, voice_output_mode: 'disabled', tts_api_protocol: 'dashscope', tts_api_base_url: 'https://dashscope.aliyuncs.com/api/v1', tts_api_model: 'qwen3-tts-flash', tts_api_key: '', tts_api_configured: false, tts_api_voice: 'Cherry', tts_api_language: 'Chinese', vits_model_name: '', vits_model_path: '', vits_speaker_id: null, vits_target_language: 'ja', vits_speed: 1, vits_emotion_params: '', vits_translate_enabled: true, microphone_device_name: null, asr_app_id: '', asr_api_key: '', asr_api_secret: '', asr_configured: false, proactive_enabled: false, proactive_min_minutes: 45, proactive_max_minutes: 120, proactive_daily_limit: 6, qdrant_url: 'http://127.0.0.1:6333', embedding_base_url: '', embedding_model: '', embedding_api_key: '', embedding_dimension: 0, memory_configured: false, memory_observer_enabled: true, memory_observer_interval: 30, tool_hook_enabled: false, tool_hook_mode: 'fixed', tool_hook_port: 34125, tool_hook_fixed_text: '你在 {tool} 里 {project} 的任务已经完成了。', tool_hook_fixed_voice_text: '', tool_hook_include_last_message: true, tool_hook_min_interval_minutes: 10, tool_hook_daily_limit: 20, tool_hook_debounce_seconds: 0, tool_hook_voice_enabled: true, system_status_enabled: false, taskbar_apps_enabled: false, now_playing_enabled: false, voice_input_mode: 'disabled', push_to_talk_shortcut: '', pet_show_on_fullscreen: true });
  let runtime = $state<RuntimeStatus | null>(null), messages = $state<Message[]>([]);
  let text = $state(''), error = $state('');
  let busy = $state(false), bubbleVisible = $state(false), composerVisible = $state(false), saved = $state(false);
  let mousePassthrough = $state(false), petHovered = $state(false);
  let bubbleDisplaySeconds = $state(Number(localStorage.getItem('bubbleDisplaySeconds') || '30'));
  let petImageSize = $state(Number(localStorage.getItem('petImageSize') || '100'));
  let bubbleTimer: number | null = null;
  type PetEmotion = 'shy' | 'affectionate' | 'sad' | 'happy' | 'calm' | 'angry' | 'battle' | 'self_deprecating';
  const petImages: Record<PetEmotion, string> = {
    shy: '/roxy-shy.png', affectionate: '/roxy-affectionate.png', sad: '/roxy-sad.png', happy: '/roxy-happy.png',
    calm: '/roxy-calm.png', angry: '/roxy-angry.png', battle: '/roxy-battle.png', self_deprecating: '/roxy-self_deprecating.png'
  };
  let petEmotion = $state<PetEmotion>('calm');
  let petImageSrc = $derived(petImages[petEmotion]);
  // 气泡出现时扩展窗口，不缩放角色图片。
  let petDisplayScale = $derived(Math.min(150, Math.max(60, petImageSize)) / 100);
  let voice = $state<VoiceStatus>({ state: 'disabled' });
  let microphones = $state<string[]>([]);
  let testingVoice = $state(false);
  let voiceInputMode = $state<'disabled' | 'continuous' | 'push_to_talk'>('disabled');
  let pushToTalkTokens = $state<string[]>(['VK:119']);
  let pushToTalkLabel = $state('F8');
  let capturingShortcut = $state(false);
  let shortcutCaptureCommittedAt = 0;
  let todos = $state<Todo[]>([]);
  let settingsTab = $state<'general' | 'toolhook'>('general');
  let toolHookTools = $state<ToolHookToolInfo[]>([]);
  let toolHookToolId = $state('opencode');
  let toolHookBusy = $state('');
  let toolHookTesting = $state(false);
  const toolHookTool = $derived(toolHookTools.find((t) => t.id === toolHookToolId));
  async function loadToolHookSupport() {
    try { toolHookTools = await listToolHookSupport(); } catch (e) { error = String(e); }
  }
  async function writeHook(itemId: string) {
    toolHookBusy = itemId; error = '';
    try { settings = await saveSettings(settings); await writeToolHookConfig(toolHookToolId, itemId); await loadToolHookSupport(); } catch (e) { error = String(e); } finally { toolHookBusy = ''; }
  }
  async function removeHook(itemId: string) {
    toolHookBusy = itemId; error = '';
    try { settings = await saveSettings(settings); await removeToolHookConfig(toolHookToolId, itemId); await loadToolHookSupport(); } catch (e) { error = String(e); } finally { toolHookBusy = ''; }
  }
  async function testHook() {
    toolHookTesting = true; error = '';
    try { settings = await saveSettings(settings); await testToolHook(toolHookToolId); } catch (e) { error = String(e); } finally { toolHookTesting = false; }
  }
  // 原生命中测试：透明像素直接穿透，气泡和输入控件保持可交互。
  const petAlphaCache = new Map<string, Promise<ImageData | null>>();
  let hitTestSyncQueued = false, hitTestSyncInFlight = false, lastHitTestLayout = '';
  function loadPetAlphaData(src: string): Promise<ImageData | null> { let cached = petAlphaCache.get(src); if (!cached) { cached = (async () => { try { const image = new Image(); image.src = src; await image.decode(); const canvas = document.createElement('canvas'); canvas.width = image.naturalWidth; canvas.height = image.naturalHeight; const context = canvas.getContext('2d', { willReadFrequently: true }); if (!context) return null; context.drawImage(image, 0, 0); return context.getImageData(0, 0, canvas.width, canvas.height); } catch { return null; } })(); petAlphaCache.set(src, cached); } return cached; }
  function alphaRuns(image: ImageData): { start: number; end: number }[][] { return Array.from({ length: image.height }, (_, y) => { const runs: { start: number; end: number }[] = []; let start = -1; for (let x = 0; x < image.width; x++) { const opaque = image.data[(y * image.width + x) * 4 + 3] > 16; if (opaque && start < 0) start = x; if (!opaque && start >= 0) { runs.push({ start, end: x }); start = -1; } } if (start >= 0) runs.push({ start, end: image.width }); return runs; }); }
  function schedulePetHitTestLayout() { if (label !== 'pet' || hitTestSyncQueued) return; hitTestSyncQueued = true; requestAnimationFrame(async () => { hitTestSyncQueued = false; if (hitTestSyncInFlight) { schedulePetHitTestLayout(); return; } hitTestSyncInFlight = true; try { await syncPetHitTestLayout(); } finally { hitTestSyncInFlight = false; } }); }
  async function syncPetHitTestLayout() { if (label !== 'pet') return; await tick(); const root = document.querySelector<HTMLElement>('.pet-window'), pet = document.querySelector<HTMLElement>('.pet'); if (!root || !pet) return; const rootRect = root.getBoundingClientRect(); const rect = (element: Element) => { const bounds = element.getBoundingClientRect(); return { x: bounds.left - rootRect.left, y: bounds.top - rootRect.top, width: bounds.width, height: bounds.height }; }; const petRect = { x: pet.offsetLeft, y: pet.offsetTop, width: pet.offsetWidth, height: pet.offsetHeight }; const interactive = [...document.querySelectorAll('.speech-bubble, .pet-composer, .mic-indicator')].map(rect); const image = await loadPetAlphaData(petImageSrc); const petMask = image ? { rect: petRect, imageWidth: image.width, imageHeight: image.height, rows: alphaRuns(image) } : { rect: petRect, imageWidth: 1, imageHeight: 1, rows: [[{ start: 0, end: 1 }]] }; const rootStyle = getComputedStyle(root); const paddingHeight = parseFloat(rootStyle.paddingTop) + parseFloat(rootStyle.paddingBottom); const flowHeight = [...root.children].reduce((total, child) => { const element = child as HTMLElement, style = getComputedStyle(element); return style.position === 'absolute' ? total : total + element.offsetHeight + parseFloat(style.marginTop) + parseFloat(style.marginBottom); }, 0); const contentHeight = Math.max(230, Math.ceil(paddingHeight + flowHeight)), scaleFactor = window.devicePixelRatio || 1; const signature = JSON.stringify({ scaleFactor, interactive, pet: petMask.rect, image: petImageSrc, contentHeight, passthrough: mousePassthrough }); if (signature === lastHitTestLayout) return; lastHitTestLayout = signature; try { await updatePetHitTestLayout({ scaleFactor, interactive, pet: petMask, contentHeight }); } catch (e) { console.warn('同步原生命中区域失败:', e); } }  function showBubble(timed = true, emotion?: string | null) {
    bubbleVisible = true;
    if (bubbleTimer !== null) window.clearTimeout(bubbleTimer);
    petEmotion = emotion && emotion in petImages ? emotion as PetEmotion : 'calm';
    bubbleTimer = timed ? window.setTimeout(() => { bubbleVisible = false; petEmotion = 'calm'; bubbleTimer = null; schedulePetHitTestLayout(); }, Math.max(1, bubbleDisplaySeconds) * 1000) : null;
    schedulePetHitTestLayout();
  }
  function hideBubble() {
    if (bubbleTimer !== null) { window.clearTimeout(bubbleTimer); bubbleTimer = null; }
    bubbleVisible = false;
    petEmotion = 'calm';
    schedulePetHitTestLayout();
  }
  function openComposer() { if (!mousePassthrough) { composerVisible = true; requestAnimationFrame(() => { document.querySelector<HTMLInputElement>('.pet-composer input')?.focus(); schedulePetHitTestLayout(); }); } }
  async function changeMousePassthrough(enabled: boolean) { await setMousePassthrough(enabled); mousePassthrough = enabled; }
  function updatePetImageSize(value: number) { petImageSize = Math.min(150, Math.max(60, value)); localStorage.setItem('petImageSize', String(petImageSize)); emit('pet-image-size-preview', petImageSize).catch(() => {}); schedulePetHitTestLayout(); }
  onMount(() => {
    if (label === 'pet') {
      Object.values(petImages).forEach((src) => { const image = new Image(); image.src = src; });
      document.documentElement.style.overflow = 'hidden';
      document.body.style.overflow = 'hidden';
    }
    const blockMenu = (event: MouseEvent) => event.preventDefault(); window.addEventListener('contextmenu', blockMenu);
    let petResizeObserver: ResizeObserver | null = null;
    if (label === 'pet') {
      const petRoot = document.querySelector<HTMLElement>('.pet-window');
      if (petRoot) { petResizeObserver = new ResizeObserver(schedulePetHitTestLayout); petResizeObserver.observe(petRoot); }
      schedulePetHitTestLayout();
    }
    let unlisteners: (() => void)[] = [];
    let menuResizeObserver: ResizeObserver | null = null;
    if (label === 'pet-menu') {
      document.documentElement.style.overflow='hidden';
      document.body.style.overflow='hidden';
      const menu=document.querySelector<HTMLElement>('.pet-menu');
      if (menu) {
        const resizeMenuWindow=()=>{
          const bounds=menu.getBoundingClientRect();
          getCurrentWindow().setSize(new LogicalSize(Math.ceil(bounds.width + 34), Math.ceil(bounds.height + 27))).catch(() => {});
        };
        menuResizeObserver=new ResizeObserver(resizeMenuWindow);
        menuResizeObserver.observe(menu);
        resizeMenuWindow();
      }
    }
    const runtimeRefresh = label === 'settings' ? window.setInterval(() => { getRuntimeStatus().then((status) => runtime = status).catch(() => {}); }, 3000) : null;
    const todoRefresh = label === 'todos' ? window.setInterval(() => { listTodos().then((items) => todos = items).catch(() => {}); }, 3000) : null;
    Promise.all([getSettings(), listMessages(), getRuntimeStatus(), listMicrophoneDevices(), getMousePassthrough(), listen<VoiceStatus>('voice-status', (event) => { voice = event.payload; }), listen<Message>('assistant-message', (event) => { messages.push(event.payload); busy = false; showBubble(true, event.payload.emotion); }), listen<string>('voice-transcript', (event) => { messages.push({ id: crypto.randomUUID(), role: 'user', content: event.payload, trigger_type: 'user_voice', created_at: Date.now() }); busy = true; showBubble(false); }), listen<boolean>('mouse-passthrough-changed', (event) => { mousePassthrough=event.payload; lastHitTestLayout=''; schedulePetHitTestLayout(); }), listen<boolean>('pet-hover-changed', (event) => { petHovered=event.payload; }), listen<number>('pet-image-size-preview', (event) => { petImageSize=event.payload; }), listen<'disabled' | 'continuous' | 'push_to_talk'>('voice-input-mode-changed', (event) => { voiceInputMode=event.payload; settings.voice_input_mode=event.payload; }), listen<boolean>('proactive-enabled-changed', (event) => { settings.proactive_enabled=event.payload; }), listen('tauri://focus', () => { if (label === 'pet-menu') getSettings().then((s) => settings=s).catch(() => {}); }), listen<CapturedBinding>('shortcut-capture-preview', (event) => { pushToTalkLabel=event.payload.label; }), listen<CapturedBinding>('shortcut-captured', (event) => { pushToTalkTokens=event.payload.tokens; pushToTalkLabel=event.payload.label; capturingShortcut=false; shortcutCaptureCommittedAt=Date.now(); settings.push_to_talk_shortcut=JSON.stringify({ tokens: pushToTalkTokens, label: pushToTalkLabel }); localStorage.setItem('bubbleDisplaySeconds', String(Math.max(1, bubbleDisplaySeconds))); })]).then(([s,m,r,devices,passthrough,...listeners]) => { settings=s; messages=m; runtime=r; microphones=devices; mousePassthrough=passthrough; voiceInputMode=s.voice_input_mode; if (s.push_to_talk_shortcut) { try { const binding = JSON.parse(s.push_to_talk_shortcut) as CapturedBinding; if (binding.tokens?.length) { pushToTalkTokens=binding.tokens; pushToTalkLabel=binding.label; } } catch {} } voice.state=r.microphone_status === 'listening' ? 'listening' : 'disabled'; unlisteners=listeners; setPushToTalkShortcut(voiceInputMode === 'push_to_talk' ? pushToTalkTokens : []).catch((e) => error=String(e)); }).catch((e) => error=String(e));
    if (label === 'todos') listTodos().then((items) => todos = items).catch((e) => error=String(e));
    if (label === 'settings') loadToolHookSupport();
    return () => { window.removeEventListener('contextmenu', blockMenu); petResizeObserver?.disconnect(); if (runtimeRefresh !== null) window.clearInterval(runtimeRefresh); if (todoRefresh !== null) window.clearInterval(todoRefresh); unlisteners.forEach((unlisten) => unlisten()); if (bubbleTimer !== null) window.clearTimeout(bubbleTimer); menuResizeObserver?.disconnect(); };
  });
  async function submit() {
    const content = text.trim(); if (!content || busy) return;
    text = ''; composerVisible = false; schedulePetHitTestLayout(); error = ''; messages.push({ id: crypto.randomUUID(), role: 'user', content, trigger_type: 'user_text', created_at: Date.now() }); busy = true; showBubble(false);
    try { const reply = await sendMessage(content); messages.push(reply); showBubble(true, reply.emotion); } catch (e) { error = String(e); showBubble(); } finally { busy = false; }
  }
  async function persist() {
    saved = false; error = '';
    try { settings.voice_input_mode = voiceInputMode; settings.push_to_talk_shortcut = JSON.stringify({ tokens: pushToTalkTokens, label: pushToTalkLabel }); settings = await saveSettings(settings); if (settings.voice_output_mode === 'gpt_sovits') await startGptSovits(); localStorage.setItem('bubbleDisplaySeconds', String(Math.max(1, bubbleDisplaySeconds))); localStorage.setItem('petImageSize', String(petImageSize)); await applyVoiceInputMode(voiceInputMode); runtime = await getRuntimeStatus(); saved = true; } catch (e) { error = String(e); }
  }
  async function toggleShortcutCapture() {
    error='';
    try {
      if (capturingShortcut || Date.now() - shortcutCaptureCommittedAt < 300) return;
      capturingShortcut=true; pushToTalkLabel='请按下新的键或鼠标键…'; await beginShortcutCapture();
    } catch(e) { capturingShortcut=false; error=String(e); }
  }  async function refreshMicrophones() { try { microphones=await listMicrophoneDevices(); } catch (e) { error=String(e); } }
  async function toggleVoice() { error=''; try { if (voice.state === 'disabled' || voice.state === 'error') { settings=await saveSettings(settings); await startVoiceListening(); } else await stopVoiceListening(); } catch (e) { error=String(e); voice={state:'error',detail:error}; } }
  async function testVoice() { testingVoice=true; error=''; try { settings=await saveSettings(settings); await testVoiceOutput(); } catch(e) { error=String(e); } finally { testingVoice=false; } }
  async function applyVoiceInputMode(next: 'disabled' | 'continuous' | 'push_to_talk') {
    error='';
    try {
      if (voice.state !== 'disabled' && voice.state !== 'error') await stopVoiceListening();
      await setPushToTalkShortcut(next === 'push_to_talk' ? pushToTalkTokens : []);
      if (next === 'continuous') await startVoiceListening();
      voiceInputMode=next;
      settings.voice_input_mode=next;
      settings.push_to_talk_shortcut=JSON.stringify({ tokens: pushToTalkTokens, label: pushToTalkLabel });
      settings=await saveSettings(settings);
      await emit('voice-input-mode-changed', next);
    } catch(e) { error=String(e); }
  }
  async function cycleVoiceInputMode() {
    const next = voiceInputMode === 'disabled' ? 'continuous' : voiceInputMode === 'continuous' ? 'push_to_talk' : 'disabled';
    await applyVoiceInputMode(next);
  }
  async function showPetMenu(event: MouseEvent) { event.preventDefault(); event.stopPropagation(); try { await openPetMenu(event.clientX,event.clientY); } catch(e) { error=String(e); } }
  async function toggleProactiveFromMenu() { error=''; try { const enabled=await toggleProactiveEnabled(); settings.proactive_enabled=enabled; } catch(e) { error=String(e); } }
  async function closeMenu() { if (label === 'pet-menu') await getCurrentWindow().hide(); }
  async function enablePassthroughFromMenu() { await changeMousePassthrough(true); await closeMenu(); }
  async function openWindow(label: 'settings'|'todos') { try { await openAppWindow(label); await closeMenu(); } catch(e) { error=String(e); } }
  const formatDue = (timestamp: number) => new Intl.DateTimeFormat('zh-CN',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(timestamp));
  const formatRepeat = (minutes: number) => minutes % 1440 === 0 ? `每 ${minutes / 1440} 天` : minutes % 60 === 0 ? `每 ${minutes / 60} 小时` : `每 ${minutes} 分钟`;
  async function removeTodo(id: string) {
    error = '';
    try { await deleteTodo(id); todos = todos.filter((todo) => todo.id !== id); } catch (e) { error = String(e); }
  }
  let systemPreview = $state('');
  let systemPreviewBusy = $state(false);
  async function previewSystemPerception() {
    systemPreviewBusy = true; error = '';
    try {
      settings = await saveSettings(settings);
      const parts: string[] = [];
      if (settings.system_status_enabled) {
        const stats = await getHardwareStats();
        let line = `硬件：CPU ${stats.cpu_usage_percent.toFixed(0)}%，内存 ${(stats.memory_used_mb / Math.max(1, stats.memory_total_mb) * 100).toFixed(0)}%（${stats.memory_used_mb} / ${stats.memory_total_mb} MB）`;
        if (stats.gpu_name) line += `；显卡 ${stats.gpu_name} ${stats.gpu_usage_percent?.toFixed(0) ?? '-'}%${stats.gpu_temperature_celsius != null ? `，${stats.gpu_temperature_celsius.toFixed(0)}℃` : ''}`;
        parts.push(line);
      }
      if (settings.taskbar_apps_enabled) {
        const apps = await getTaskbarApps();
        parts.push(apps.length ? `任务栏应用：${apps.map((a) => `${a.title}（${a.process_name}）`).join('、')}` : '任务栏应用：未枚举到窗口');
        const foreground = apps.find((a) => a.foreground);
        if (foreground) parts.push(`当前前台：${foreground.title}（${foreground.process_name}）`);
      }
      if (settings.now_playing_enabled) {
        const now = await getNowPlaying();
        parts.push(now ? `正在播放：${now.title} - ${now.artist}（${now.source_app}，${now.playing ? '播放中' : '已暂停'}）` : '当前播放：未检测到媒体会话');
      }
      systemPreview = parts.join('\n') || '请先开启上方至少一项感知开关';
    } catch (e) { error = String(e); } finally { systemPreviewBusy = false; }
  }
</script>

{#if label === 'pet-menu'}
  <main class="menu-window">
    <div class="pet-menu" role="menu">
      <button onclick={cycleVoiceInputMode}><Mic size={15}/><span>语音输入</span><em>{voiceInputMode === 'disabled' ? '关' : voiceInputMode === 'continuous' ? '持续' : '按键'}</em></button>
      <button onclick={toggleProactiveFromMenu}><MessageCircle size={15}/><span>主动对话</span><em>{settings.proactive_enabled ? '开' : '关'}</em></button>
      <hr/>
      <button onclick={enablePassthroughFromMenu}><MousePointer2 size={15}/><span>启用鼠标穿透</span></button>
      <button onclick={()=>openWindow('settings')}><Settings size={15}/><span>打开设置</span></button>
      <button onclick={()=>openWindow('todos')}><ListTodo size={15}/><span>查看待办</span></button>
    </div>
  </main>
{:else if label === 'settings'}
  <main class="settings-page">
    <header><span>CHATPET</span><h1>设置</h1></header>
    <nav class="settings-tabs">
      <button class:on={settingsTab === 'general'} onclick={() => settingsTab = 'general'}>常规</button>
      <button class:on={settingsTab === 'toolhook'} onclick={() => { settingsTab = 'toolhook'; loadToolHookSupport(); }}>编程联动</button>
    </nav>
    {#if settingsTab === 'general'}
    <section><h2>角色</h2>
      <label>如何称呼你<input bind:value={settings.user_name} /></label>
      <label>角色图片大小 <span class="range-value">{petImageSize}%</span><input class="size-slider" type="range" min="60" max="150" step="1" value={petImageSize} oninput={(event) => updatePetImageSize(Number(event.currentTarget.value))} /></label>
      <label>对话气泡显示时间（秒）<input type="number" min="1" max="600" bind:value={bubbleDisplaySeconds} /></label>
      <p class="note">回复显示超过该时间后自动隐藏，默认 30 秒。气泡右上角也可以手动关闭。</p>
      <button class="option" class:on={settings.pet_show_on_fullscreen} onclick={() => settings.pet_show_on_fullscreen = !settings.pet_show_on_fullscreen}>
        <span><strong>在全屏应用上显示桌宠</strong><small>{settings.pet_show_on_fullscreen ? '全屏游戏 / 视频时桌宠仍保持显示' : '桌宠所在屏幕出现全屏应用时自动隐藏，退出全屏后恢复'}</small></span>
      </button>
    </section>
    <section><h2>主动陪伴</h2>
      <button class="option" class:on={settings.proactive_enabled} onclick={() => settings.proactive_enabled = !settings.proactive_enabled}>
        <span><strong>主动消息</strong><small>{settings.proactive_enabled ? '在随机间隔到达时由桌宠主动发起消息' : '已关闭'}</small></span>
      </button>
      <div class="interval-grid">
        <label>最短间隔（分钟）<input type="number" min="1" max="10080" bind:value={settings.proactive_min_minutes} disabled={!settings.proactive_enabled} /></label>
        <label>最长间隔（分钟）<input type="number" min="1" max="10080" bind:value={settings.proactive_max_minutes} disabled={!settings.proactive_enabled} /></label>
        <label>每日上限<input type="number" min="1" max="100" bind:value={settings.proactive_daily_limit} disabled={!settings.proactive_enabled} /></label>
      </div>
      <p class="note">保存后重新随机计算下一次时间。关闭应用期间的主动消息不会补发；到点提醒仍会在下次启动后补发。</p>
    </section>
    <section><h2>模型 API</h2>
      <div class="model-summary"><strong>{settings.api_model || '未配置'}</strong><small>{settings.api_base_url}</small><span class:ready={settings.api_key_configured}>{settings.api_key_configured ? '已从 .env 读取 API Key' : '.env 缺少 API Key'}</span></div>
      <p class="note">模型连接由项目根目录的 .env 管理。修改后请完全退出并重新启动 ChatPet。</p>
    </section>
    <section><h2>长期记忆</h2>
      <div class="model-summary"><strong>本地长期记忆</strong><small>SQLite 存储 · 观察者独占维护</small><span class:ready={runtime?.memory_status === 'available'}>{runtime?.memory_status === 'available' ? '可用' : '尚未配置 Embedding'}</span></div>
      <label>Embedding API 地址<input bind:value={settings.embedding_base_url} placeholder="OpenAI-compatible base URL" /></label>
      <label>Embedding 模型<input bind:value={settings.embedding_model} /></label>
      <label>Embedding 维度<input type="number" min="1" bind:value={settings.embedding_dimension} /></label>
      <label>Embedding API Key<input type="password" bind:value={settings.embedding_api_key} autocomplete="new-password" placeholder={settings.memory_configured ? '留空则保持现有配置' : '请输入 API Key'} /></label>
      <button class="option" class:on={settings.memory_observer_enabled} onclick={() => settings.memory_observer_enabled = !settings.memory_observer_enabled}>
        <span><strong>记忆观察者</strong><small>{settings.memory_observer_enabled ? '定期总结对话中值得长期保存的信息' : '已关闭'}</small></span>
      </button>
      <label>每多少条消息观察一次<input type="number" min="2" max="500" bind:value={settings.memory_observer_interval} disabled={!settings.memory_observer_enabled} /></label>
      <p class="note">记忆事实与向量都保存在本地 SQLite；语义检索由 Rust 在进程内完成。Embedding 不可用时自动退化为普通对话。</p>
    </section>
    <section><h2>系统感知</h2>
      <button class="option" class:on={settings.system_status_enabled} onclick={() => settings.system_status_enabled = !settings.system_status_enabled}>
        <span><strong>硬件状态</strong><small>{settings.system_status_enabled ? '允许洛琪希查看 CPU、内存与显卡占用' : '已关闭'}</small></span>
      </button>
      <button class="option" class:on={settings.taskbar_apps_enabled} onclick={() => settings.taskbar_apps_enabled = !settings.taskbar_apps_enabled}>
        <span><strong>任务栏应用</strong><small>{settings.taskbar_apps_enabled ? '允许洛琪希查看任务栏正在运行的应用和当前前台窗口' : '已关闭'}</small></span>
      </button>
      <button class="option" class:on={settings.now_playing_enabled} onclick={() => settings.now_playing_enabled = !settings.now_playing_enabled}>
        <span><strong>当前播放</strong><small>{settings.now_playing_enabled ? '允许洛琪希查看系统正在播放的音乐（QQ音乐需在客户端设置中开启 SMTC）' : '已关闭'}</small></span>
      </button>
      {#if settings.system_status_enabled || settings.taskbar_apps_enabled || settings.now_playing_enabled}
        <button type="button" class="test-button" onclick={previewSystemPerception} disabled={systemPreviewBusy}>{systemPreviewBusy ? '正在读取…' : '预览当前状态'}</button>
        {#if systemPreview}<p class="note system-preview">{systemPreview}</p>{/if}
      {/if}
      <p class="note">均为只读能力，逐项授权、默认关闭。开启后洛琪希会在对话中参考对应信息回答（如“电脑卡不卡”“我在听什么歌”）；数据只在本机实时读取，不保存、不上传。</p>
    </section>
    <section><h2>可选能力</h2>
      <div class="voice-settings">
        <div class="model-summary"><strong>语音输出</strong><small>洛琪希专用 GPT-SoVITS</small><span class:ready={settings.voice_output_mode === 'gpt_sovits'}>{settings.voice_output_mode === 'gpt_sovits' ? 'GPT-SoVITS' : '关闭'}</span></div>
        <div class="voice-modes">
          <label><input type="radio" bind:group={settings.voice_output_mode} value="disabled" />不启用语音</label>
          <label><input type="radio" bind:group={settings.voice_output_mode} value="gpt_sovits" />GPT-SoVITS</label>
        </div>
        {#if settings.voice_output_mode === 'gpt_sovits'}
          <div class="model-summary"><strong>GPT-SoVITS v2ProPlus</strong><small>{runtime?.voice_gpu ?? '仅支持 NVIDIA GPU'}</small><span class:ready={runtime?.vits_status === 'available'}>{runtime?.vits_status === 'available' ? '已就绪' : runtime?.vits_status === 'starting' ? '启动中' : runtime?.vits_status === 'unsupported_gpu' ? '不支持' : runtime?.vits_status === 'error' ? '启动失败' : '未安装扩展'}</span></div>
          <p class="note">{runtime?.voice_hardware_detail ?? '正在检测显卡与语音扩展…'}</p>
          <p class="note">使用回复中的日文与情绪自动选择参考音频。</p>
          <button type="button" class="test-button" onclick={testVoice} disabled={testingVoice}>{testingVoice ? '正在生成并播放…' : '保存配置并测试语音'}</button>
        {/if}
      </div>      <div class="asr-settings">
        <div class="model-summary"><strong>讯飞流式语音识别</strong><span class:ready={settings.asr_configured}>{settings.asr_configured ? '凭据已配置' : '尚未配置完整凭据'}</span></div>
        <label>APPID<input bind:value={settings.asr_app_id} autocomplete="off" /></label>
        <label>APIKey<input type="password" bind:value={settings.asr_api_key} autocomplete="new-password" placeholder={settings.asr_configured ? '留空则保持现有配置' : '请输入 APIKey'} /></label>
        <label>APISecret<input type="password" bind:value={settings.asr_api_secret} autocomplete="new-password" placeholder={settings.asr_configured ? '留空则保持现有配置' : '请输入 APISecret'} /></label>
        <p class="note">优先使用应用内保存的值；留空时使用项目 .env。Key 和 Secret 不会在页面中回显。</p>
      </div>
      <label>麦克风设备
        <span class="device-row"><select bind:value={settings.microphone_device_name} disabled={voice.state !== 'disabled' && voice.state !== 'error'}><option value={null}>系统默认（跟随系统设置）</option>{#each microphones as device}<option value={device}>{device}</option>{/each}</select><button type="button" onclick={refreshMicrophones} disabled={voice.state !== 'disabled' && voice.state !== 'error'}>刷新</button></span>
      </label>
      <div class="voice-settings">
        <div class="model-summary"><strong>语音输入方式</strong></div>
        <div class="voice-modes">
          <label><input type="radio" bind:group={voiceInputMode} value="disabled" />关闭</label>
          <label><input type="radio" bind:group={voiceInputMode} value="continuous" />持续监听</label>
          <label><input type="radio" bind:group={voiceInputMode} value="push_to_talk" />按住说话</label>
        </div>
        {#if voiceInputMode === 'push_to_talk'}
          <label>全局按键<button type="button" class:capturing={capturingShortcut} class="shortcut-capture" onclick={toggleShortcutCapture}>{pushToTalkLabel}</button></label>
          <p class="note">最好不要使用容易和系统或者其它应用冲突的快捷键</p>
        {:else if voiceInputMode === 'continuous'}
          <button class="option" class:on={voice.state !== 'disabled' && voice.state !== 'error'} onclick={toggleVoice}>
            {#if voice.state === 'disabled' || voice.state === 'error'}<MicOff size={18} />{:else}<Mic size={18} />{/if}
            <span><strong>持续语音监听</strong><small>{voice.state === 'speaking' ? '检测到语音' : voice.state === 'recognizing' ? '正在调用讯飞识别…' : voice.state === 'thinking' ? `识别结果：${voice.detail ?? ''}` : voice.state === 'listening' ? `正在监听${voice.detail ? `：${voice.detail}` : ''}` : voice.detail || '已关闭'}</small></span>
          </button>
        {/if}
      </div>
    </section>
    {:else}
    <section><h2>编程联动提醒</h2>
      <button class="option" class:on={settings.tool_hook_enabled} onclick={() => settings.tool_hook_enabled = !settings.tool_hook_enabled}>
        <span><strong>启用编程联动提醒</strong><small>{settings.tool_hook_enabled ? '监听已接入编程工具的任务完成事件并主动提醒' : '已关闭'}</small></span>
      </button>
      <label>软件
        <select bind:value={toolHookToolId} onchange={() => loadToolHookSupport()}>
          {#each toolHookTools as tool}<option value={tool.id}>{tool.name}</option>{/each}
        </select>
      </label>
      {#each toolHookTool?.items ?? [] as item (item.id)}
        <div class="tool-hook-item">
          <div class="tool-hook-row"><strong>{item.label}</strong><span class="status-badge" class:configured={item.status === 'configured'} class:needs_update={item.status === 'needs_update'} class:error={item.status === 'error'}>{item.status === 'configured' ? '已配置' : item.status === 'needs_update' ? '需更新' : item.status === 'error' ? '异常' : '未配置'}</span></div>
          <p class="note">{item.detail}</p>
          <div class="tool-hook-actions">
            <button type="button" class="test-button" onclick={() => writeHook(item.id)} disabled={toolHookBusy === item.id || item.status === 'configured'}>一键配置</button>
            <button type="button" class="ghost-button" onclick={() => removeHook(item.id)} disabled={toolHookBusy === item.id || item.status === 'not_configured'}>删除配置</button>
          </div>
        </div>
      {/each}
      <p class="note">一键配置会写入该软件的 hook 配置，删除配置会恢复原状。写入后需重启对应软件生效（Codex 首次还需在 /hooks 里信任一次）。</p>
    </section>
    <section><h2>提醒方式</h2>
      <div class="voice-modes">
        <label><input type="radio" bind:group={settings.tool_hook_mode} value="fixed" />固定提示（不调用 AI）</label>
        <label><input type="radio" bind:group={settings.tool_hook_mode} value="ai" />消息提示（AI 动态生成）</label>
      </div>
      {#if settings.tool_hook_mode === 'fixed'}
        <label>固定提示文本<textarea bind:value={settings.tool_hook_fixed_text} placeholder={'支持 {tool} {project} 占位符'}></textarea></label>
        <label>语音文本（日文，GPT-SoVITS 用）<input bind:value={settings.tool_hook_fixed_voice_text} placeholder="可留空" /></label>
      {:else}
        <button class="option" class:on={settings.tool_hook_include_last_message} onclick={() => settings.tool_hook_include_last_message = !settings.tool_hook_include_last_message}>
          <span><strong>附带最后一条助手消息摘要</strong><small>{settings.tool_hook_include_last_message ? 'AI 会参考该摘要生成提示' : '仅提供工具与项目信息，更保守'}</small></span>
        </button>
      {/if}
      <button class="option" class:on={settings.tool_hook_voice_enabled} onclick={() => settings.tool_hook_voice_enabled = !settings.tool_hook_voice_enabled}>
        <span><strong>语音播报</strong><small>{settings.tool_hook_voice_enabled ? '提示同时朗读' : '仅显示气泡'}</small></span>
      </button>
    </section>
    <section><h2>通知策略</h2>
      <div class="interval-grid">
        <label>提醒间隔（分钟，0 为无间隔）<input type="number" min="0" max="10080" bind:value={settings.tool_hook_min_interval_minutes} /></label>
        <label>每日上限（0 为不限）<input type="number" min="0" max="100" bind:value={settings.tool_hook_daily_limit} /></label>
        <label>去抖秒数<input type="number" min="0" max="3600" bind:value={settings.tool_hook_debounce_seconds} /></label>
      </div>
      <p class="note">提醒间隔为同一工具+项目两次提醒的最短间隔，0 表示每次完成都提醒；每日上限 0 表示不限次数。去抖用于把多轮连续执行合并为一次提醒。</p>
    </section>
    <section><h2>监听</h2>
      <label>端口<input type="number" min="1" max="65535" bind:value={settings.tool_hook_port} /></label>
      <button type="button" class="test-button" onclick={testHook} disabled={toolHookTesting}>{toolHookTesting ? '正在模拟…' : '模拟发送一次事件'}</button>
      <p class="note">修改端口或提示设置后请点击底部「保存」生效；保存时会自动重写已过期的一键配置。仅监听本机回环地址（127.0.0.1）。</p>
    </section>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}{#if saved}<p class="success">设置已保存</p>{/if}
    <footer><button class="save" onclick={persist}>保存</button></footer>
  </main>
{:else if label === 'todos'}
  <main class="todos-page">
    <header><span>CHATPET</span><h1>待办提醒</h1><small>{todos.length} 项未完成</small></header>
    {#if error}<p class="error">{error}</p>{/if}
    {#if todos.length === 0}<div class="todo-empty"><ListTodo size={30}/><strong>目前没有待办</strong><span>对桌宠说“提醒我……”即可创建。</span></div>{:else}
      <div class="todo-list">{#each todos as todo (todo.id)}<article class:overdue={todo.due_at_utc < Date.now()}><i></i><div><strong>{todo.title}</strong><span>{formatDue(todo.due_at_utc)} · {todo.timezone}{#if todo.repeat_interval_minutes} · <em class="repeat-badge">{formatRepeat(todo.repeat_interval_minutes)}</em>{/if}</span></div><button class="todo-delete" aria-label="删除待办" title="删除" onclick={() => removeTodo(todo.id)}><Trash2 size={14}/></button></article>{/each}</div>
    {/if}
  </main>
{:else}
  <main class="pet-window" data-tauri-drag-region>
    {#if bubbleVisible || busy || error}
      <div class="speech-bubble" ><p>{error || (busy ? '…………' : messages.at(-1)?.content || '我在这里。')}</p><button class="bubble-close" aria-label="关闭气泡" onclick={hideBubble}><X size={13} /></button></div>
    {/if}
    <button class="pet" class:hovered={petHovered} style={`width:${172 * petDisplayScale}px;height:${198 * petDisplayScale}px`} aria-label="洛琪希，双击输入消息" ondblclick={openComposer} oncontextmenu={showPetMenu} data-tauri-drag-region><img src={petImageSrc} alt="洛琪希" draggable="false" /></button>
    {#if composerVisible}
      <form class="pet-composer" onsubmit={(e) => { e.preventDefault(); submit(); }}>
        <input bind:value={text} onkeydown={(e) => { if (e.key === 'Escape') { composerVisible=false; schedulePetHitTestLayout(); } }} placeholder="和洛琪希说点什么…" />
        <button disabled={!text.trim() || busy} aria-label="发送"><Send size={16} /></button>
        <button type="button" aria-label="关闭输入" onclick={() => { composerVisible=false; schedulePetHitTestLayout(); }}><X size={16} /></button>
      </form>
    {/if}
    {#if voice.state !== 'disabled'}<i class="mic-indicator" class:speaking={voice.state === 'speaking'} title={voice.detail || voice.state}></i>{/if}

  </main>
{/if}

<style>
  .device-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 7px; }
  .device-row select { min-width: 0; border: 1px solid #d3c6b8; border-radius: 9px; padding: 9px 10px; color: #39332d; background: #fffaf3; }
  .device-row button { border: 1px solid #d3c6b8; border-radius: 9px; padding: 0 12px; color: #5e554d; background: #fffaf3; }
  .device-row :disabled { cursor: default; opacity: .55; }
  .mic-indicator { position: absolute; right: 36px; bottom: 30px; width: 9px; height: 9px; border-radius: 50%; background: #6ca46f; box-shadow: 0 0 0 4px rgba(108,164,111,.18); }
  .mic-indicator.speaking { background: #d57954; box-shadow: 0 0 0 5px rgba(213,121,84,.22); }
  .asr-settings { display: grid; gap: 10px; padding: 12px; border: 1px solid #e1d5c8; border-radius: 12px; background: rgba(255,250,243,.55); }
  .interval-grid { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: 8px; }
  .voice-settings { display:grid; gap:10px; padding:12px; border:1px solid #e1d5c8; border-radius:12px; background:rgba(255,250,243,.55); }
  .voice-modes { display:flex; flex-wrap:wrap; gap:14px; }
  .voice-modes label { display:flex; grid-template-columns:none; align-items:center; gap:6px; margin:0; }
  .voice-modes input { width:auto; }
  .shortcut-capture { width:100%; border:1px solid #d3c6b8; border-radius:9px; padding:10px; color:#513f34; background:#fffaf3; text-align:left; }
  .shortcut-capture.capturing { border-color:#a05f43; background:#f5e5d7; color:#8d482f; }  .test-button { justify-self:start; border:0; border-radius:9px; padding:8px 13px; color:white; background:#8d563f; }
  .test-button:disabled { opacity:.55; }
  .menu-window { display:flow-root; width:max-content; min-height:0; padding:10px 17px 17px; overflow:hidden; background:transparent; }
  .menu-window .pet-menu { position:static; width:max-content; min-width:166px; transform:none; }
  .pet-menu { position:fixed; z-index:20; width:166px; padding:5px; border:1px solid #d8cab9; border-radius:11px; background:#fffaf2; box-shadow:0 12px 30px rgba(50,35,24,.2); transform:translate(-4px,-100%); }
  .pet-menu button { width:100%; display:grid; grid-template-columns:18px max-content auto; align-items:center; gap:6px; padding:8px; border:0; border-radius:7px; color:#433a32; text-align:left; white-space:nowrap; background:transparent; }
  .pet-menu button > span { min-width:0; }
  .pet-menu button:hover { background:#eee0d2; }
  .pet-menu em { color:#9b6047; font-size:11px; font-style:normal; }
  .pet-menu hr { margin:4px 5px; border:0; border-top:1px solid #e3d8cb; }
  .todos-page { min-height:100vh; padding:27px 30px; color:#39332d; background:#f5f0e8; }
  .todos-page header { position:relative; margin-bottom:20px; }
  .todos-page header>span { color:#a05f43; font-size:10px; font-weight:700; letter-spacing:.18em; }
  .todos-page h1 { margin:3px 0 0; font:600 26px Georgia,"Microsoft YaHei UI",serif; }
  .todos-page header small { position:absolute; right:0; bottom:4px; color:#81766b; }
  .todo-list { display:grid; gap:9px; }
  .todo-list article { display:flex; gap:11px; align-items:flex-start; padding:13px; border:1px solid #ded2c4; border-radius:11px; background:#fffaf3; }
  .todo-list article>i { flex:0 0 auto; width:9px; height:9px; margin-top:5px; border:2px solid #a87861; border-radius:50%; }
  .todo-list article.overdue>i { border-color:#c45e50; background:#c45e50; }
  .todo-list article div { display:grid; gap:4px; }
  .todo-list article strong { font-size:13px; line-height:1.45; }
  .todo-list article span { color:#81766b; font-size:11px; }
  .todo-empty { min-height:310px; display:grid; place-content:center; justify-items:center; gap:7px; color:#81766b; text-align:center; }
  .todo-empty strong { color:#554a41; }
  .todo-empty span { font-size:12px; }
  .todo-list article { align-items:center; }
  .todo-list article div { flex:1; min-width:0; }
  .todo-delete { flex:0 0 auto; display:grid; place-items:center; width:26px; height:26px; border:0; border-radius:7px; color:#9a8c7d; background:transparent; cursor:pointer; }
  .todo-delete:hover { color:#b0493f; background:#f3e4de; }
  .repeat-badge { color:#8d563f; font-style:normal; }
  .bubble-close { position:absolute; top:6px; right:8px; z-index:2; display:grid; place-items:center; width:20px; height:20px; padding:0; border:0; border-radius:50%; color:#9a8c7d; background:transparent; cursor:pointer; }
  .bubble-close:hover { color:#5e4a3c; background:#f0e6da; }
  .range-value { justify-self: end; margin-top: -20px; color: #9b6047; font-variant-numeric: tabular-nums; }
  .settings-page input.size-slider { padding: 0; accent-color: #9b6047; cursor: pointer; }
  .pet-window { position: relative; pointer-events: none; }
  .pet { width: 172px; height: 198px; flex-shrink: 0; pointer-events: auto; }
  .speech-bubble { position: relative; flex-shrink: 0; width: calc(100% - 28px); margin: 0 14px 2px; padding: 13px 16px; border: 1px solid rgba(190,177,163,.85); border-radius: 18px; color: #39332d; background: #fff; box-shadow: 0 10px 26px rgba(55,40,28,.15); pointer-events: auto; }
  .speech-bubble::after { content: ''; position: absolute; left: 61%; bottom: -13px; width: 22px; height: 22px; border-right: 1px solid rgba(190,177,163,.85); border-bottom: 1px solid rgba(190,177,163,.85); background: #fff; transform: skew(-20deg) rotate(45deg); }
  .speech-bubble p { position: relative; z-index: 1; margin: 0; padding-right: 16px; overflow-wrap: anywhere; font-size: 13px; line-height: 1.6; white-space: pre-wrap; }
  .pet-composer { position: absolute; z-index: 12; left: 22px; right: 22px; bottom: 76px; display: grid; grid-template-columns: minmax(0,1fr) 34px 34px; gap: 6px; padding: 8px; border: 1px solid #d8cab9; border-radius: 13px; background: rgba(255,250,242,.97); box-shadow: 0 12px 28px rgba(55,40,28,.2); pointer-events: auto; }
  .pet-composer input { min-width: 0; border: 1px solid #ddd0c0; border-radius: 8px; padding: 8px 10px; outline: none; background: #fff; }
  .pet-composer button { display: grid; place-items: center; padding: 0; border: 0; border-radius: 8px; color: #fff; background: #9b6047; }
  .pet-composer button[type='button'] { color: #75685c; background: #eee5db; }
  .pet-composer button:disabled { opacity: .35; }
  .pet-menu, .mic-indicator { pointer-events: auto; }
  .settings-tabs { display: flex; gap: 4px; margin: -6px 0 0; border-bottom: 1px solid #dcd1c4; }
  .settings-tabs button { border: 0; background: transparent; padding: 9px 15px; font-size: 13px; color: #6b5f53; border-bottom: 2px solid transparent; }
  .settings-tabs button.on { color: #8d482f; border-bottom-color: #9b6047; font-weight: 650; }
  .tool-hook-item { padding: 12px; border: 1px solid #e1d5c8; border-radius: 12px; background: rgba(255,250,243,.55); margin-bottom: 10px; }
  .tool-hook-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .tool-hook-row strong { font-size: 13px; }
  .status-badge { padding: 3px 10px; border-radius: 999px; font-size: 11px; background: #e6ded2; color: #6b5f53; white-space: nowrap; }
  .status-badge.configured { background: #dfeadd; color: #3c673c; }
  .status-badge.needs_update { background: #f6e7cf; color: #9a6b1f; }
  .status-badge.error { background: #f3dddd; color: #803b37; }
  .tool-hook-actions { display: flex; gap: 8px; margin-top: 8px; }
  .ghost-button { border: 1px solid #d3c6b8; border-radius: 9px; padding: 8px 13px; color: #5e554d; background: #fffaf3; }
  .ghost-button:disabled { opacity: .5; cursor: default; }
  .settings-page select { width: 100%; border: 1px solid #d3c6b8; border-radius: 9px; padding: 9px 10px; color: #39332d; background: #fffaf3; }
  .system-preview { white-space: pre-wrap; overflow-wrap: anywhere; }</style>
