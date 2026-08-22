<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import { emit, listen } from '@tauri-apps/api/event';
  import { ListTodo, MessageCircle, Mic, MicOff, MousePointer2, Send, Settings, X } from 'lucide-svelte';
  import { getRuntimeStatus, getSettings, listMessages, saveSettings, sendMessage, startVoiceListening, stopVoiceListening, listMicrophoneDevices, testVoiceOutput, startGptSovits, listTodos, openAppWindow, openPetMenu, toggleProactiveEnabled, setPushToTalkShortcut, beginShortcutCapture, cancelShortcutCapture, setMousePassthrough, getMousePassthrough } from './lib/api';
  import type { AppSettings, Message, RuntimeStatus, Todo, VoiceStatus } from './lib/types';
  import type { CapturedBinding } from './lib/api';
  const label = '__TAURI_INTERNALS__' in window ? getCurrentWindow().label : new URLSearchParams(location.search).get('view') ?? 'pet';
  let settings = $state<AppSettings>({ pet_name: 'ChatPet', persona: '', user_name: '你', api_base_url: '', api_model: '', api_key_configured: false, voice_output_enabled: false, voice_output_mode: 'disabled', tts_api_protocol: 'dashscope', tts_api_base_url: 'https://dashscope.aliyuncs.com/api/v1', tts_api_model: 'qwen3-tts-flash', tts_api_key: '', tts_api_configured: false, tts_api_voice: 'Cherry', tts_api_language: 'Chinese', vits_model_name: '', vits_model_path: '', vits_speaker_id: null, vits_target_language: 'ja', vits_speed: 1, vits_emotion_params: '', vits_translate_enabled: true, microphone_device_name: null, asr_app_id: '', asr_api_key: '', asr_api_secret: '', asr_configured: false, proactive_enabled: false, proactive_min_minutes: 45, proactive_max_minutes: 120, proactive_daily_limit: 6, qdrant_url: 'http://127.0.0.1:6333', embedding_base_url: '', embedding_model: '', embedding_api_key: '', embedding_dimension: 0, memory_configured: false, memory_observer_enabled: true, memory_observer_interval: 30 });
  let runtime = $state<RuntimeStatus | null>(null), messages = $state<Message[]>([]);
  let text = $state(''), error = $state('');
  let busy = $state(false), bubbleVisible = $state(false), composerVisible = $state(false), saved = $state(false);
  let mousePassthrough = $state(false);
  let proactiveEnabled = $state(false);
  let bubbleDisplaySeconds = $state(Number(localStorage.getItem('bubbleDisplaySeconds') || '30'));
  let petImageSize = $state(Number(localStorage.getItem('petImageSize') || '100'));
  let bubbleTimer: number | null = null;
  let voice = $state<VoiceStatus>({ state: 'disabled' });
  let microphones = $state<string[]>([]);
  let testingVoice = $state(false);
  let voiceInputMode = $state<'disabled' | 'continuous' | 'push_to_talk'>((localStorage.getItem('voiceInputMode') as 'disabled' | 'continuous' | 'push_to_talk') || 'disabled');
  let pushToTalkTokens = $state<string[]>(JSON.parse(localStorage.getItem('pushToTalkTokens') || '["VK:119"]'));
  let pushToTalkLabel = $state(localStorage.getItem('pushToTalkLabel') || 'F8');
  let capturingShortcut = $state(false);
  let shortcutCaptureCommittedAt = 0;
  let todos = $state<Todo[]>([]);
  function showBubble(timed = true) {
    bubbleVisible = true;
    if (bubbleTimer !== null) window.clearTimeout(bubbleTimer);
    bubbleTimer = timed ? window.setTimeout(() => { bubbleVisible = false; bubbleTimer = null; }, Math.max(1, bubbleDisplaySeconds) * 1000) : null;
  }
  function openComposer() { if (!mousePassthrough) { composerVisible = true; requestAnimationFrame(() => document.querySelector<HTMLInputElement>('.pet-composer input')?.focus()); } }
  async function changeMousePassthrough(enabled: boolean) { await setMousePassthrough(enabled); mousePassthrough = enabled; }
  function updatePetImageSize(value: number) { petImageSize = Math.min(150, Math.max(60, value)); localStorage.setItem('petImageSize', String(petImageSize)); emit('pet-image-size-preview', petImageSize).catch(() => {}); }
  onMount(() => {
    const blockMenu = (event: MouseEvent) => event.preventDefault(); window.addEventListener('contextmenu', blockMenu);
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
    Promise.all([getSettings(), listMessages(), getRuntimeStatus(), listMicrophoneDevices(), getMousePassthrough(), listen<VoiceStatus>('voice-status', (event) => { voice = event.payload; }), listen<Message>('assistant-message', (event) => { messages.push(event.payload); busy = false; showBubble(); }), listen<string>('voice-transcript', (event) => { messages.push({ id: crypto.randomUUID(), role: 'user', content: event.payload, trigger_type: 'user_voice', created_at: Date.now() }); busy = true; showBubble(false); }), listen<boolean>('mouse-passthrough-changed', (event) => { mousePassthrough=event.payload; }), listen<number>('pet-image-size-preview', (event) => { petImageSize=event.payload; }), listen<'disabled' | 'continuous' | 'push_to_talk'>('voice-input-mode-changed', (event) => { voiceInputMode=event.payload; localStorage.setItem('voiceInputMode', event.payload); }), listen<boolean>('proactive-enabled-changed', (event) => { proactiveEnabled=event.payload; settings.proactive_enabled=event.payload; }), listen<CapturedBinding>('shortcut-capture-preview', (event) => { pushToTalkLabel=event.payload.label; }), listen<CapturedBinding>('shortcut-captured', (event) => { pushToTalkTokens=event.payload.tokens; pushToTalkLabel=event.payload.label; capturingShortcut=false; shortcutCaptureCommittedAt=Date.now(); localStorage.setItem('pushToTalkTokens', JSON.stringify(pushToTalkTokens)); localStorage.setItem('pushToTalkLabel', pushToTalkLabel); localStorage.setItem('bubbleDisplaySeconds', String(Math.max(1, bubbleDisplaySeconds))); })]).then(([s,m,r,devices,passthrough,...listeners]) => { settings=s; proactiveEnabled=s.proactive_enabled; messages=m; runtime=r; microphones=devices; mousePassthrough=passthrough; voice.state=r.microphone_status === 'listening' ? 'listening' : 'disabled'; unlisteners=listeners; setPushToTalkShortcut(voiceInputMode === 'push_to_talk' ? pushToTalkTokens : []).catch((e) => error=String(e)); }).catch((e) => error=String(e));
    if (label === 'todos') listTodos().then((items) => todos = items).catch((e) => error=String(e));
    return () => { window.removeEventListener('contextmenu', blockMenu); if (runtimeRefresh !== null) window.clearInterval(runtimeRefresh); if (todoRefresh !== null) window.clearInterval(todoRefresh); unlisteners.forEach((unlisten) => unlisten()); if (bubbleTimer !== null) window.clearTimeout(bubbleTimer); menuResizeObserver?.disconnect(); };
  });
  async function submit() {
    const content = text.trim(); if (!content || busy) return;
    text = ''; composerVisible = false; error = ''; messages.push({ id: crypto.randomUUID(), role: 'user', content, trigger_type: 'user_text', created_at: Date.now() }); busy = true; showBubble(false);
    try { messages.push(await sendMessage(content)); showBubble(); } catch (e) { error = String(e); showBubble(); } finally { busy = false; }
  }
  async function persist() {
    saved = false; error = '';
    try { settings = await saveSettings(settings); if (settings.voice_output_mode === 'gpt_sovits') await startGptSovits(); localStorage.setItem('pushToTalkTokens', JSON.stringify(pushToTalkTokens)); localStorage.setItem('pushToTalkLabel', pushToTalkLabel); localStorage.setItem('bubbleDisplaySeconds', String(Math.max(1, bubbleDisplaySeconds))); localStorage.setItem('petImageSize', String(petImageSize)); await applyVoiceInputMode(voiceInputMode); runtime = await getRuntimeStatus(); saved = true; } catch (e) { error = String(e); }
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
      localStorage.setItem('voiceInputMode', next);
      await emit('voice-input-mode-changed', next);
    } catch(e) { error=String(e); }
  }
  async function cycleVoiceInputMode() {
    const next = voiceInputMode === 'disabled' ? 'continuous' : voiceInputMode === 'continuous' ? 'push_to_talk' : 'disabled';
    await applyVoiceInputMode(next);
  }
  async function showPetMenu(event: MouseEvent) { event.preventDefault(); event.stopPropagation(); try { await openPetMenu(event.clientX,event.clientY); } catch(e) { error=String(e); } }
  async function toggleProactiveFromMenu() { error=''; try { proactiveEnabled=await toggleProactiveEnabled(); settings.proactive_enabled=proactiveEnabled; await emit('proactive-enabled-changed', proactiveEnabled); } catch(e) { error=String(e); } }
  async function closeMenu() { if (label === 'pet-menu') await getCurrentWindow().hide(); }
  async function enablePassthroughFromMenu() { await changeMousePassthrough(true); await closeMenu(); }
  async function openWindow(label: 'settings'|'todos') { try { await openAppWindow(label); await closeMenu(); } catch(e) { error=String(e); } }
  const formatDue = (timestamp: number) => new Intl.DateTimeFormat('zh-CN',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(timestamp));
</script>

{#if label === 'pet-menu'}
  <main class="menu-window">
    <div class="pet-menu" role="menu">
      <button onclick={cycleVoiceInputMode}><Mic size={15}/><span>语音输入</span><em>{voiceInputMode === 'disabled' ? '关' : voiceInputMode === 'continuous' ? '持续' : '按键'}</em></button>
      <button onclick={toggleProactiveFromMenu}><MessageCircle size={15}/><span>主动对话</span><em>{proactiveEnabled ? '开' : '关'}</em></button>
      <hr/>
      <button onclick={enablePassthroughFromMenu}><MousePointer2 size={15}/><span>启用鼠标穿透</span></button>
      <button onclick={()=>openWindow('settings')}><Settings size={15}/><span>打开设置</span></button>
      <button onclick={()=>openWindow('todos')}><ListTodo size={15}/><span>查看待办</span></button>
    </div>
  </main>
{:else if label === 'settings'}
  <main class="settings-page">
    <header><span>CHATPET</span><h1>设置</h1></header>
    <section><h2>角色</h2>
      <label>如何称呼你<input bind:value={settings.user_name} /></label>
      <label>角色图片大小 <span class="range-value">{petImageSize}%</span><input class="size-slider" type="range" min="60" max="150" step="1" value={petImageSize} oninput={(event) => updatePetImageSize(Number(event.currentTarget.value))} /></label>
      <label>对话气泡显示时间（秒）<input type="number" min="1" max="600" bind:value={bubbleDisplaySeconds} /></label>
      <p class="note">回复显示超过该时间后自动隐藏，默认 30 秒。</p>
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
      <div class="model-summary"><strong>Qdrant + Embedding</strong><small>Runtime：{runtime?.qdrant_runtime_status ?? 'unknown'}</small><span class:ready={runtime?.memory_status === 'available'}>{runtime?.memory_status === 'available' ? '可用' : runtime?.memory_status === 'unavailable' ? 'Qdrant 暂不可用' : '尚未配置'}</span></div>
      <label>Qdrant 地址<input bind:value={settings.qdrant_url} placeholder="http://127.0.0.1:6333" /></label>
      <label>Embedding API 地址<input bind:value={settings.embedding_base_url} placeholder="OpenAI-compatible base URL" /></label>
      <label>Embedding 模型<input bind:value={settings.embedding_model} /></label>
      <label>Embedding 维度<input type="number" min="1" bind:value={settings.embedding_dimension} /></label>
      <label>Embedding API Key<input type="password" bind:value={settings.embedding_api_key} autocomplete="new-password" placeholder={settings.memory_configured ? '留空则保持现有配置' : '请输入 API Key'} /></label>
      <button class="option" class:on={settings.memory_observer_enabled} onclick={() => settings.memory_observer_enabled = !settings.memory_observer_enabled}>
        <span><strong>记忆观察者</strong><small>{settings.memory_observer_enabled ? '定期总结对话中值得长期保存的信息' : '已关闭'}</small></span>
      </button>
      <label>每多少条消息观察一次<input type="number" min="2" max="500" bind:value={settings.memory_observer_interval} disabled={!settings.memory_observer_enabled} /></label>
      <p class="note">SQLite 保存记忆事实，Qdrant 仅负责语义检索。服务不可用时自动退化为普通对话。</p>
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
        <span class="device-row"><select bind:value={settings.microphone_device_name} disabled={voice.state !== 'disabled' && voice.state !== 'error'}><option value={null}>请选择麦克风</option>{#each microphones as device}<option value={device}>{device}</option>{/each}</select><button type="button" onclick={refreshMicrophones} disabled={voice.state !== 'disabled' && voice.state !== 'error'}>刷新</button></span>
      </label>
      <div class="voice-settings">
        <div class="model-summary"><strong>语音输入方式</strong><small>按键模式不会加载 Silero VAD</small></div>
        <div class="voice-modes">
          <label><input type="radio" bind:group={voiceInputMode} value="disabled" />关闭</label>
          <label><input type="radio" bind:group={voiceInputMode} value="continuous" />持续监听</label>
          <label><input type="radio" bind:group={voiceInputMode} value="push_to_talk" />按住说话</label>
        </div>
        {#if voiceInputMode === 'push_to_talk'}
          <label>全局按键<button type="button" class:capturing={capturingShortcut} class="shortcut-capture" onclick={toggleShortcutCapture}>{pushToTalkLabel}</button></label>
          <p class="note">点击按钮后按下任意键盘键或鼠标按钮；组合中的所有键全部松开后自动确认。也可以再次点击该按钮，将鼠标左键设为触发键。</p>
        {:else if voiceInputMode === 'continuous'}
          <button class="option" class:on={voice.state !== 'disabled' && voice.state !== 'error'} onclick={toggleVoice} disabled={!settings.microphone_device_name}>
            {#if voice.state === 'disabled' || voice.state === 'error'}<MicOff size={18} />{:else}<Mic size={18} />{/if}
            <span><strong>持续语音监听</strong><small>{voice.state === 'speaking' ? '检测到语音' : voice.state === 'recognizing' ? '正在调用讯飞识别…' : voice.state === 'thinking' ? `识别结果：${voice.detail ?? ''}` : voice.state === 'listening' ? `正在监听${voice.detail ? `：${voice.detail}` : ''}` : voice.detail || '已关闭'}</small></span>
          </button>
        {/if}
      </div>
    </section>
    {#if error}<p class="error">{error}</p>{/if}{#if saved}<p class="success">设置已保存</p>{/if}
    <footer><button class="save" onclick={persist}>保存</button></footer>
  </main>
{:else if label === 'todos'}
  <main class="todos-page">
    <header><span>CHATPET</span><h1>待办提醒</h1><small>{todos.length} 项未完成</small></header>
    {#if error}<p class="error">{error}</p>{/if}
    {#if todos.length === 0}<div class="todo-empty"><ListTodo size={30}/><strong>目前没有待办</strong><span>对桌宠说“提醒我……”即可创建。</span></div>{:else}
      <div class="todo-list">{#each todos as todo}<article class:overdue={todo.due_at_utc < Date.now()}><i></i><div><strong>{todo.title}</strong><span>{formatDue(todo.due_at_utc)} · {todo.timezone}</span></div></article>{/each}</div>
    {/if}
  </main>
{:else}
  <main class="pet-window" data-tauri-drag-region>
    {#if bubbleVisible || busy || error}
      <div class="speech-bubble"><p>{error || (busy ? '让我想一想…' : messages.at(-1)?.content || '我在这里。')}</p></div>
    {/if}
    <button class="pet" style={`width:${172 * petImageSize / 100}px;height:${198 * petImageSize / 100}px`} aria-label="洛琪希，双击输入消息" ondblclick={openComposer} oncontextmenu={showPetMenu} data-tauri-drag-region><img src="/roxy-idle.png" alt="洛琪希" draggable="false" /></button>
    {#if composerVisible}
      <form class="pet-composer" onsubmit={(e) => { e.preventDefault(); submit(); }}>
        <input bind:value={text} onkeydown={(e) => { if (e.key === 'Escape') composerVisible=false; }} placeholder="和洛琪希说点什么…" />
        <button disabled={!text.trim() || busy} aria-label="发送"><Send size={16} /></button>
        <button type="button" aria-label="关闭输入" onclick={() => composerVisible=false}><X size={16} /></button>
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
  .range-value { justify-self: end; margin-top: -20px; color: #9b6047; font-variant-numeric: tabular-nums; }
  .settings-page input.size-slider { padding: 0; accent-color: #9b6047; cursor: pointer; }
  .pet-window { position: relative; pointer-events: none; }
  .pet { width: 172px; height: 198px; pointer-events: auto; }
  .speech-bubble { position: relative; width: calc(100% - 28px); margin: 0 14px 2px; padding: 13px 16px; border: 1px solid rgba(190,177,163,.85); border-radius: 18px; color: #39332d; background: #fff; box-shadow: 0 10px 26px rgba(55,40,28,.15); pointer-events: auto; }
  .speech-bubble::after { content: ''; position: absolute; left: 61%; bottom: -13px; width: 22px; height: 22px; border-right: 1px solid rgba(190,177,163,.85); border-bottom: 1px solid rgba(190,177,163,.85); background: #fff; transform: skew(-20deg) rotate(45deg); }
  .speech-bubble p { position: relative; z-index: 1; max-height: 98px; margin: 0; overflow: auto; font-size: 13px; line-height: 1.6; white-space: pre-wrap; }
  .pet-composer { position: absolute; z-index: 12; left: 22px; right: 22px; bottom: 76px; display: grid; grid-template-columns: minmax(0,1fr) 34px 34px; gap: 6px; padding: 8px; border: 1px solid #d8cab9; border-radius: 13px; background: rgba(255,250,242,.97); box-shadow: 0 12px 28px rgba(55,40,28,.2); pointer-events: auto; }
  .pet-composer input { min-width: 0; border: 1px solid #ddd0c0; border-radius: 8px; padding: 8px 10px; outline: none; background: #fff; }
  .pet-composer button { display: grid; place-items: center; padding: 0; border: 0; border-radius: 8px; color: #fff; background: #9b6047; }
  .pet-composer button[type='button'] { color: #75685c; background: #eee5db; }
  .pet-composer button:disabled { opacity: .35; }
  .pet-menu, .mic-indicator { pointer-events: auto; }</style>
