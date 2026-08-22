import { invoke } from '@tauri-apps/api/core';
import type { AppSettings, Message, RuntimeStatus, Todo, VitsModelInfo } from './types';

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
  qdrant_url: 'http://127.0.0.1:6333', embedding_base_url: '', embedding_model: '', embedding_api_key: '', embedding_dimension: 0, memory_configured: false, memory_observer_enabled: true, memory_observer_interval: 30
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

export async function sendMessage(content: string): Promise<Message> {
  if (!inTauri()) {
    await new Promise((resolve) => setTimeout(resolve, 450));
    return { id: crypto.randomUUID(), role: 'assistant', content: `我听见了：“${content}”。配置模型后，我会用自己的性格认真回答。`, trigger_type: 'user_text', created_at: Date.now() };
  }
  return invoke('send_text_message', { content });
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  if (!inTauri()) return { database_ready: true, llm_configured: false, vits_status: 'not_configured', memory_status: 'not_configured', microphone_status: 'disabled', qdrant_runtime_status: 'not_started', voice_hardware_status: 'unsupported', voice_hardware_detail: '仅支持 NVIDIA GPU', voice_gpu: null };
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
export async function openAppWindow(label: 'settings' | 'todos'): Promise<void> { if (inTauri()) await invoke('open_app_window', { label }); }
export async function openPetMenu(x: number, y: number): Promise<void> { if (inTauri()) await invoke('show_pet_menu', { x, y }); }
export async function toggleProactiveEnabled(): Promise<boolean> { return inTauri() ? invoke('toggle_proactive_enabled') : true; }

export async function setMousePassthrough(enabled: boolean): Promise<void> { if (inTauri()) await invoke('set_mouse_passthrough', { enabled }); }
export async function getMousePassthrough(): Promise<boolean> { return inTauri() ? invoke('get_mouse_passthrough') : false; }
