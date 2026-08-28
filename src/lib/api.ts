import { invoke } from '@tauri-apps/api/core';
import type { AppSettings, HardwareStats, McpServerRequest, McpServerStatus, McpToolInfo, Message, NowPlaying, RuntimeStatus, TaskbarApp, Todo, ToolHookStatus, ToolHookToolInfo, VitsModelInfo } from './types';

const inTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const demoSettings: AppSettings = {
  pet_name: '小栞',
  persona: '温柔、克制、偶尔有一点俏皮的陪伴者。',
  user_name: '你',
  api_base_url: '',
  api_model: '',
  api_key_configured: false,
  voice_output_enabled: false,
  voice_output_mode: 'disabled', tts_api_protocol: 'dashscope', tts_api_base_url: 'https://dashscope.aliyuncs.com/api/v1', tts_api_model: 'qwen3-tts-flash', tts_api_key: '', tts_api_configured: false, tts_api_voice: 'Cherry', tts_api_language: 'Chinese', vits_model_name: '', vits_model_path: '', vits_speaker_id: null, vits_target_language: 'ja', vits_speed: 1, vits_emotion_params: '', vits_translate_enabled: true,
  microphone_device_name: null,
  asr_app_id: '',
  asr_api_key: '',
  asr_api_secret: '',
  asr_configured: false,
  proactive_enabled: false,
  proactive_min_minutes: 45,
  proactive_max_minutes: 120,
  proactive_daily_limit: 6,
  qdrant_url: 'http://127.0.0.1:6333', embedding_base_url: '', embedding_model: '', embedding_api_key: '', embedding_dimension: 0, memory_configured: false, memory_observer_enabled: true, memory_observer_interval: 30,
  tool_hook_enabled: false, tool_hook_mode: 'fixed', tool_hook_port: 34125, tool_hook_tool_texts: {}, tool_hook_include_last_message: true, tool_hook_min_interval_minutes: 10, tool_hook_daily_limit: 20, tool_hook_debounce_seconds: 0, tool_hook_voice_enabled: true,
  system_status_enabled: false, taskbar_apps_enabled: false, now_playing_enabled: false,
  voice_input_mode: 'disabled', push_to_talk_shortcut: '', pet_show_on_fullscreen: true,
  search_provider: '', search_api_key: '', search_base_url: '', search_api_configured: false,
  agent_max_tool_rounds: 30
};

export async function getSettings(): Promise<AppSettings> {
  return inTauri() ? invoke('get_settings') : demoSettings;
}

export async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  if (!inTauri()) return settings;
  return invoke('save_settings', { request: settings });
}

export async function listMessages(): Promise<Message[]> {
  return inTauri() ? invoke('list_messages', { limit: 100 }) : [];
}

// 历史会话页：分页加载主会话（用户输入 + 主会话回复），before 为上一页最旧一条的 created_at。
export async function listMainSessionHistory(before?: number, limit = 50): Promise<Message[]> {
  return inTauri() ? invoke('list_main_session_history', { before: before ?? null, limit }) : [];
}

export async function sendMessage(content: string): Promise<Message> {
  if (!inTauri()) {
    await new Promise((resolve) => setTimeout(resolve, 450));
    return { id: crypto.randomUUID(), role: 'assistant', content: `我听见了：“${content}”。配置模型后，我会用自己的性格认真回答。`, trigger_type: 'user_text', created_at: Date.now() };
  }
  return invoke('send_text_message', { content });
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  if (!inTauri()) return { database_ready: true, llm_configured: false, vits_status: 'not_configured', memory_status: 'not_configured', microphone_status: 'disabled', voice_hardware_status: 'unsupported', voice_hardware_detail: '仅支持 NVIDIA GPU', voice_gpu: null };
  return invoke('get_runtime_status');
}

export async function startVoiceListening(): Promise<void> { if (inTauri()) await invoke('start_voice_listening'); }
export async function stopVoiceListening(): Promise<void> { if (inTauri()) await invoke('stop_voice_listening'); }
export async function listMicrophoneDevices(): Promise<string[]> { return inTauri() ? invoke('list_microphone_devices') : []; }
export interface CapturedBinding { tokens: string[]; label: string; }
export async function setPushToTalkShortcut(tokens: string[]): Promise<CapturedBinding> { return inTauri() ? invoke('set_push_to_talk_shortcut', { tokens }) : { tokens, label: tokens.join(' + ') }; }
export async function beginShortcutCapture(): Promise<void> { if (inTauri()) await invoke('begin_shortcut_capture'); }
export async function cancelShortcutCapture(): Promise<void> { if (inTauri()) await invoke('cancel_shortcut_capture'); }
export async function scanVitsModels(): Promise<VitsModelInfo[]> { return inTauri() ? invoke('scan_vits_models') : []; }
export async function testVoiceOutput(): Promise<void> { if (inTauri()) await invoke('test_voice_output'); }
export async function startGptSovits(): Promise<void> { if (inTauri()) await invoke('start_gpt_sovits'); }
export async function listTodos(): Promise<Todo[]> { return inTauri() ? invoke('list_todos') : []; }
export async function deleteTodo(id: string): Promise<boolean> { return inTauri() ? invoke('delete_todo', { id }) : true; }
export async function openAppWindow(label: 'settings' | 'todos' | 'history'): Promise<void> { if (inTauri()) await invoke('open_app_window', { label }); }
export async function openPetMenu(x: number, y: number): Promise<void> { if (inTauri()) await invoke('show_pet_menu', { x, y }); }
export async function toggleProactiveEnabled(): Promise<boolean> { return inTauri() ? invoke('toggle_proactive_enabled') : true; }

export async function setMousePassthrough(enabled: boolean): Promise<void> { if (inTauri()) await invoke('set_mouse_passthrough', { enabled }); }
export async function getMousePassthrough(): Promise<boolean> { return inTauri() ? invoke('get_mouse_passthrough') : false; }

export interface PetHitTestRect { x: number; y: number; width: number; height: number; }
export interface PetHitTestAlphaRun { start: number; end: number; }
export interface PetHitTestLayout {
  scaleFactor: number;
  interactive: PetHitTestRect[];
  pet: { rect: PetHitTestRect; imageWidth: number; imageHeight: number; rows: PetHitTestAlphaRun[][] } | null;
  contentHeight: number;
}
export async function updatePetHitTestLayout(request: PetHitTestLayout): Promise<void> {
  if (inTauri()) await invoke('update_pet_hit_test_layout', { request });
}

export async function listToolHookSupport(): Promise<ToolHookToolInfo[]> { return inTauri() ? invoke('list_tool_hook_support') : []; }
export async function writeToolHookConfig(tool: string, item: string): Promise<ToolHookStatus> { return inTauri() ? invoke('write_tool_hook_config', { tool, item }) : { status: 'not_configured', detail: '非 Tauri 环境' }; }
export async function removeToolHookConfig(tool: string, item: string): Promise<ToolHookStatus> { return inTauri() ? invoke('remove_tool_hook_config', { tool, item }) : { status: 'not_configured', detail: '非 Tauri 环境' }; }
export async function testToolHook(tool: string): Promise<void> { if (inTauri()) await invoke('test_tool_hook', { tool }); }

export async function listMcpServers(): Promise<McpServerStatus[]> { return inTauri() ? invoke('list_mcp_servers') : []; }
export async function addMcpServer(request: McpServerRequest): Promise<McpServerStatus> {
  if (!inTauri()) return { id: Date.now(), ...request, enabled: true, created_at: Date.now(), updated_at: Date.now(), status: 'pending_restart', tool_count: 0, requires_restart: request.transport === 'stdio', error: null };
  return invoke('add_mcp_server', { request });
}
export async function updateMcpServer(id: number, request: McpServerRequest): Promise<McpServerStatus> {
  if (!inTauri()) return { id, ...request, enabled: true, created_at: Date.now(), updated_at: Date.now(), status: 'pending_restart', tool_count: 0, requires_restart: request.transport === 'stdio', error: null };
  return invoke('update_mcp_server', { id, request });
}
export async function removeMcpServer(id: number): Promise<boolean> { return inTauri() ? invoke('remove_mcp_server', { id }) : true; }
export async function setMcpServerEnabled(id: number, enabled: boolean): Promise<McpServerStatus | null> { return inTauri() ? invoke('set_mcp_server_enabled', { id, enabled }) : null; }
export async function listMcpServerTools(id: number): Promise<McpToolInfo[]> { return inTauri() ? invoke('list_mcp_server_tools', { id }) : []; }

export async function getTaskbarApps(): Promise<TaskbarApp[]> { return inTauri() ? invoke('get_taskbar_apps') : []; }
export async function getHardwareStats(): Promise<HardwareStats> { return inTauri() ? invoke('get_hardware_stats') : { cpu_usage_percent: 0, memory_used_mb: 0, memory_total_mb: 0, gpu_name: null, gpu_usage_percent: null, gpu_memory_used_mb: null, gpu_memory_total_mb: null, gpu_temperature_celsius: null }; }
export async function getNowPlaying(): Promise<NowPlaying | null> { return inTauri() ? invoke('get_now_playing') : null; }
