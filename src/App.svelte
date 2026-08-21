<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { ListTodo, MessageCircle, Mic, MicOff, Send, Settings, X } from 'lucide-svelte';
  import { getRuntimeStatus, getSettings, listMessages, saveSettings, sendMessage, startVoiceListening, stopVoiceListening, listMicrophoneDevices, scanVitsModels, testVoiceOutput, listTodos, openAppWindow } from './lib/api';
  import type { AppSettings, Message, RuntimeStatus, Todo, VoiceStatus, VitsModelInfo } from './lib/types';
  const label = '__TAURI_INTERNALS__' in window ? getCurrentWindow().label : new URLSearchParams(location.search).get('view') ?? 'pet';
  let settings = $state<AppSettings>({ pet_name: 'ChatPet', persona: '', user_name: '你', api_base_url: '', api_model: '', api_key_configured: false, voice_output_enabled: false, voice_output_mode: 'disabled', tts_api_protocol: 'dashscope', tts_api_base_url: 'https://dashscope.aliyuncs.com/api/v1', tts_api_model: 'qwen3-tts-flash', tts_api_key: '', tts_api_configured: false, tts_api_voice: 'Cherry', tts_api_language: 'Chinese', vits_model_name: '', vits_model_path: '', vits_speaker_id: null, vits_target_language: 'ja', vits_speed: 1, vits_emotion_params: '', vits_translate_enabled: true, microphone_device_name: null, asr_app_id: '', asr_api_key: '', asr_api_secret: '', asr_configured: false, proactive_enabled: false, proactive_min_minutes: 45, proactive_max_minutes: 120, proactive_daily_limit: 6, qdrant_url: 'http://127.0.0.1:6333', embedding_base_url: '', embedding_model: '', embedding_api_key: '', embedding_dimension: 0, memory_configured: false, memory_observer_enabled: true, memory_observer_interval: 30 });
  let runtime = $state<RuntimeStatus | null>(null), messages = $state<Message[]>([]);
  let text = $state(''), error = $state('');
  let busy = $state(false), expanded = $state(false), saved = $state(false);
  let voice = $state<VoiceStatus>({ state: 'disabled' });
  let microphones = $state<string[]>([]);
  let vitsModels = $state<VitsModelInfo[]>([]), testingVoice = $state(false);
  let todos = $state<Todo[]>([]), contextMenu = $state<{ x: number; y: number } | null>(null);
  onMount(() => {
    const blockMenu = (event: MouseEvent) => event.preventDefault(); window.addEventListener('contextmenu', blockMenu);
    const closeContextMenu = () => contextMenu = null; window.addEventListener('click', closeContextMenu);
    let unlisteners: (() => void)[] = [];
    const runtimeRefresh = label === 'settings' ? window.setInterval(() => { getRuntimeStatus().then((status) => runtime = status).catch(() => {}); }, 3000) : null;
    const todoRefresh = label === 'todos' ? window.setInterval(() => { listTodos().then((items) => todos = items).catch(() => {}); }, 3000) : null;
    Promise.all([getSettings(), listMessages(), getRuntimeStatus(), listMicrophoneDevices(), scanVitsModels(), listen<VoiceStatus>('voice-status', (event) => { voice = event.payload; }), listen<Message>('assistant-message', (event) => { messages.push(event.payload); expanded = true; busy = false; }), listen<string>('voice-transcript', (event) => { messages.push({ id: crypto.randomUUID(), role: 'user', content: event.payload, trigger_type: 'user_voice', created_at: Date.now() }); expanded = true; busy = true; })]).then(([s,m,r,devices,models,...listeners]) => { settings=s; messages=m; runtime=r; microphones=devices; vitsModels=models; voice.state=r.microphone_status === 'listening' ? 'listening' : 'disabled'; unlisteners=listeners; }).catch((e) => error=String(e));
    if (label === 'todos') listTodos().then((items) => todos = items).catch((e) => error=String(e));
    return () => { window.removeEventListener('contextmenu', blockMenu); window.removeEventListener('click', closeContextMenu); if (runtimeRefresh !== null) window.clearInterval(runtimeRefresh); if (todoRefresh !== null) window.clearInterval(todoRefresh); unlisteners.forEach((unlisten) => unlisten()); };
  });
  async function submit() {
    const content = text.trim(); if (!content || busy) return;
    text = ''; error = ''; messages.push({ id: crypto.randomUUID(), role: 'user', content, trigger_type: 'user_text', created_at: Date.now() }); busy = true;
    try { messages.push(await sendMessage(content)); } catch (e) { error = String(e); } finally { busy = false; }
  }
  async function persist() {
    saved = false; error = '';
    try { settings = await saveSettings(settings); runtime = await getRuntimeStatus(); saved = true; } catch (e) { error = String(e); }
  }
  async function refreshMicrophones() { try { microphones=await listMicrophoneDevices(); } catch (e) { error=String(e); } }
  async function toggleVoice() { error=''; try { if (voice.state === 'disabled' || voice.state === 'error') { settings=await saveSettings(settings); await startVoiceListening(); } else await stopVoiceListening(); } catch (e) { error=String(e); voice={state:'error',detail:error}; } }
  function selectVitsModel() { const model=vitsModels.find((m)=>m.name===settings.vits_model_name); settings.vits_model_path=model?.path ?? ''; settings.vits_speaker_id=null; }
  async function testVoice() { testingVoice=true; error=''; try { settings=await saveSettings(settings); await testVoiceOutput(); } catch(e) { error=String(e); } finally { testingVoice=false; } }
  function showPetMenu(event: MouseEvent) { event.preventDefault(); event.stopPropagation(); contextMenu={ x:event.clientX, y:event.clientY }; }
  async function toggleProactiveFromMenu() { contextMenu=null; error=''; try { settings.proactive_enabled=!settings.proactive_enabled; settings=await saveSettings(settings); } catch(e) { settings.proactive_enabled=!settings.proactive_enabled; error=String(e); } }
  async function toggleMicrophoneFromMenu() { contextMenu=null; await toggleVoice(); }
  async function openWindow(label: 'settings'|'todos') { contextMenu=null; try { await openAppWindow(label); } catch(e) { error=String(e); } }
  const formatDue = (timestamp: number) => new Intl.DateTimeFormat('zh-CN',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(timestamp));
</script>

{#if label === 'settings'}
  <main class="settings-page">
    <header><span>CHATPET</span><h1>设置</h1></header>
    <section><h2>角色</h2>
      <label>桌宠名字<input bind:value={settings.pet_name} /></label>
      <label>如何称呼你<input bind:value={settings.user_name} /></label>
      <label>角色人设<textarea bind:value={settings.persona} rows="5"></textarea></label>
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
        <div class="model-summary"><strong>语音输出</strong><small>API TTS 与本地 VITS 只能选择一种</small><span class:ready={settings.voice_output_mode !== 'disabled'}>{settings.voice_output_mode === 'api' ? '通用 API' : settings.voice_output_mode === 'vits' ? '本地 VITS' : '关闭'}</span></div>
        <div class="voice-modes">
          <label><input type="radio" bind:group={settings.voice_output_mode} value="disabled" />关闭</label>
          <label><input type="radio" bind:group={settings.voice_output_mode} value="api" />通用 TTS API</label>
          <label><input type="radio" bind:group={settings.voice_output_mode} value="vits" />本地 VITS</label>
        </div>
        {#if settings.voice_output_mode === 'api'}
          <label>接口协议<select bind:value={settings.tts_api_protocol}><option value="dashscope">千问 DashScope</option><option value="openai">OpenAI-compatible /audio/speech</option></select></label>
          <label>API 地址<input bind:value={settings.tts_api_base_url} placeholder="https://dashscope.aliyuncs.com/api/v1" /></label>
          <label>模型<input bind:value={settings.tts_api_model} placeholder="qwen3-tts-flash" /></label>
          <label>音色<input bind:value={settings.tts_api_voice} placeholder="Cherry" /></label>
          {#if settings.tts_api_protocol === 'dashscope'}<label>文本语言<input bind:value={settings.tts_api_language} placeholder="Chinese" /></label>{/if}
          <label>API Key<input type="password" bind:value={settings.tts_api_key} autocomplete="new-password" placeholder={settings.tts_api_configured ? '留空则保持现有配置' : '请输入 API Key'} /></label>
          <p class="note">千问非实时 TTS 返回音频 URL 或 Base64；应用会下载到本地缓存后播放。OpenAI-compatible 模式要求接口直接返回音频字节。</p>
        {:else if settings.voice_output_mode === 'vits'}
          <div class="model-summary"><strong>VITS Runtime</strong><small>data\vits_runtime\vits_runtime.exe</small><span class:ready={runtime?.vits_status === 'available'}>{runtime?.vits_status === 'available' ? '已就绪' : '未找到时自动跳过语音'}</span></div>
          <label>VITS 模型<select bind:value={settings.vits_model_name} onchange={selectVitsModel}><option value="">请选择 data\vits_models 下的模型</option>{#each vitsModels as model}<option value={model.name} disabled={!model.has_config}>{model.name}{model.language ? `（${model.language}）` : ''}</option>{/each}</select></label>
          {@const selectedVits = vitsModels.find((m)=>m.name===settings.vits_model_name)}
          {#if selectedVits?.speakers.length}<label>说话人<select bind:value={settings.vits_speaker_id}><option value={null}>默认</option>{#each selectedVits.speakers as speaker}<option value={speaker}>{speaker}</option>{/each}</select></label>{/if}
          <label>输出语言<select bind:value={settings.vits_target_language}><option value="ja">日语</option><option value="zh">中文</option><option value="en">英语</option></select></label>
          <label>语速：{settings.vits_speed.toFixed(1)}x<input type="range" min="0.5" max="2" step="0.1" bind:value={settings.vits_speed} /></label>
          <label>情感参数<input bind:value={settings.vits_emotion_params} placeholder="留空使用模型默认" /></label>
          <label class="check-row"><input type="checkbox" bind:checked={settings.vits_translate_enabled} />合成前使用 DeepSeek 按角色人设翻译</label>
          <p class="note">翻译是独立模型调用，会注入角色人设和长期记忆，不写入对话历史。中文文本、日语语音时请保持开启。</p>
        {/if}
        {#if settings.voice_output_mode !== 'disabled'}<button type="button" class="test-button" onclick={testVoice} disabled={testingVoice}>{testingVoice ? '正在生成并播放…' : '保存配置并测试语音'}</button>{/if}
      </div>
      <div class="asr-settings">
        <div class="model-summary"><strong>讯飞流式语音识别</strong><span class:ready={settings.asr_configured}>{settings.asr_configured ? '凭据已配置' : '尚未配置完整凭据'}</span></div>
        <label>APPID<input bind:value={settings.asr_app_id} autocomplete="off" /></label>
        <label>APIKey<input type="password" bind:value={settings.asr_api_key} autocomplete="new-password" placeholder={settings.asr_configured ? '留空则保持现有配置' : '请输入 APIKey'} /></label>
        <label>APISecret<input type="password" bind:value={settings.asr_api_secret} autocomplete="new-password" placeholder={settings.asr_configured ? '留空则保持现有配置' : '请输入 APISecret'} /></label>
        <p class="note">优先使用应用内保存的值；留空时使用项目 .env。Key 和 Secret 不会在页面中回显。</p>
      </div>
      <label>麦克风设备
        <span class="device-row"><select bind:value={settings.microphone_device_name} disabled={voice.state !== 'disabled' && voice.state !== 'error'}><option value={null}>请选择麦克风</option>{#each microphones as device}<option value={device}>{device}</option>{/each}</select><button type="button" onclick={refreshMicrophones} disabled={voice.state !== 'disabled' && voice.state !== 'error'}>刷新</button></span>
      </label>
      <button class="option" class:on={voice.state !== 'disabled' && voice.state !== 'error'} onclick={toggleVoice} disabled={!settings.microphone_device_name}>
        {#if voice.state === 'disabled' || voice.state === 'error'}<MicOff size={18} />{:else}<Mic size={18} />{/if}
        <span><strong>持续语音监听</strong><small>{voice.state === 'speaking' ? '检测到语音' : voice.state === 'recognizing' ? '正在调用讯飞识别…' : voice.state === 'thinking' ? `识别结果：${voice.detail ?? ''}` : voice.state === 'listening' ? `正在监听${voice.detail ? `：${voice.detail}` : ''}` : voice.detail || '已关闭'}</small></span>
      </button>
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
    {#if expanded || busy || error}
      <div class="speech">
        <button class="close" aria-label="关闭输入" onclick={() => { expanded = false; error = ''; }}><X size={14} /></button>
        <p>{error || (busy ? '让我想一想…' : messages.at(-1)?.content || '我在这里。')}</p>
        <form onsubmit={(e) => { e.preventDefault(); submit(); }}><input bind:value={text} placeholder="说点什么…" /><button disabled={!text.trim() || busy} aria-label="发送"><Send size={15} /></button></form>
      </div>
    {/if}
    <button class="pet" aria-label={`和${settings.pet_name}说话`} onclick={() => expanded = true} oncontextmenu={showPetMenu} data-tauri-drag-region><img src="/pet-placeholder.svg" alt={settings.pet_name} draggable="false" /></button>
    <span class="pet-name">{settings.pet_name}</span>
    {#if voice.state !== 'disabled'}<i class="mic-indicator" class:speaking={voice.state === 'speaking'} title={voice.detail || voice.state}></i>{/if}
    {#if contextMenu}<div class="pet-menu" role="menu" style={`left:${contextMenu.x}px;top:${contextMenu.y}px`}>
      <button onclick={toggleMicrophoneFromMenu}>{#if voice.state === 'disabled' || voice.state === 'error'}<MicOff size={15}/>{:else}<Mic size={15}/>{/if}<span>语音输入</span><em>{voice.state === 'disabled' || voice.state === 'error' ? '关' : '开'}</em></button>
      <button onclick={toggleProactiveFromMenu}><MessageCircle size={15}/><span>主动对话</span><em>{settings.proactive_enabled ? '开' : '关'}</em></button>
      <hr/>
      <button onclick={()=>openWindow('settings')}><Settings size={15}/><span>打开设置</span></button>
      <button onclick={()=>openWindow('todos')}><ListTodo size={15}/><span>查看待办</span></button>
    </div>{/if}
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
  .voice-settings select { width:100%; border:1px solid #d3c6b8; border-radius:9px; padding:9px 10px; color:#39332d; background:#fffaf3; }
  .voice-modes { display:flex; flex-wrap:wrap; gap:14px; }
  .voice-modes label,.check-row { display:flex; grid-template-columns:none; align-items:center; gap:6px; margin:0; }
  .voice-modes input,.check-row input { width:auto; }
  .test-button { justify-self:start; border:0; border-radius:9px; padding:8px 13px; color:white; background:#8d563f; }
  .test-button:disabled { opacity:.55; }
  .pet-menu { position:fixed; z-index:20; width:166px; padding:5px; border:1px solid #d8cab9; border-radius:11px; background:#fffaf2; box-shadow:0 12px 30px rgba(50,35,24,.2); transform:translate(-4px,-100%); }
  .pet-menu button { width:100%; display:grid; grid-template-columns:18px 1fr auto; align-items:center; gap:6px; padding:8px; border:0; border-radius:7px; color:#433a32; text-align:left; background:transparent; }
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
</style>
