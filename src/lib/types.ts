export interface AppSettings {
  pet_name: string;
  persona: string;
  user_name: string;
  api_base_url: string;
  api_model: string;
  api_key_configured: boolean;
  voice_output_enabled: boolean;
  voice_output_mode: 'disabled' | 'gpt_sovits';
  tts_api_protocol: 'dashscope' | 'openai';
  tts_api_base_url: string;
  tts_api_model: string;
  tts_api_key: string;
  tts_api_configured: boolean;
  tts_api_voice: string;
  tts_api_language: string;
  vits_model_name: string;
  vits_model_path: string;
  vits_speaker_id: string | null;
  vits_target_language: string;
  vits_speed: number;
  vits_emotion_params: string;
  vits_translate_enabled: boolean;
  microphone_device_name: string | null;
  asr_app_id: string;
  asr_api_key: string;
  asr_api_secret: string;
  asr_configured: boolean;
  proactive_enabled: boolean;
  proactive_min_minutes: number;
  proactive_max_minutes: number;
  proactive_daily_limit: number;
  qdrant_url: string;
  embedding_base_url: string;
  embedding_model: string;
  embedding_api_key: string;
  embedding_dimension: number;
  memory_configured: boolean;
  memory_observer_enabled: boolean;
  memory_observer_interval: number;
  tool_hook_enabled: boolean;
  tool_hook_mode: 'fixed' | 'ai';
  tool_hook_port: number;
  tool_hook_token_enabled: boolean;
  tool_hook_fixed_text: string;
  tool_hook_fixed_voice_text: string;
  tool_hook_include_last_message: boolean;
  tool_hook_min_interval_minutes: number;
  tool_hook_daily_limit: number;
  tool_hook_debounce_seconds: number;
  tool_hook_voice_enabled: boolean;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  japanese_text?: string | null;
  emotion?: string | null;
  trigger_type: string;
  created_at: number;
}

export interface RuntimeStatus {
  database_ready: boolean;
  llm_configured: boolean;
  vits_status: 'disabled' | 'not_configured' | 'unsupported_gpu' | 'starting' | 'available' | 'error';
  memory_status: 'not_configured' | 'unavailable' | 'available';
  microphone_status: 'disabled' | 'listening' | 'error';
  qdrant_runtime_status: string;
  voice_hardware_status: 'supported' | 'unsupported';
  voice_hardware_detail: string;
  voice_gpu: string | null;
}

export interface VoiceStatus { state: 'disabled' | 'listening' | 'speaking' | 'recognizing' | 'thinking' | 'error'; detail?: string; duration_ms?: number; }

export interface VitsModelInfo { name: string; path: string; language: string | null; speakers: string[]; has_config: boolean; }

export interface Todo { id: string; title: string; due_at_utc: number; timezone: string; status: string; created_at: number; }

export interface ToolHookStatus { status: 'not_configured' | 'configured' | 'needs_update' | 'error'; detail: string; }
export interface ToolHookItemInfo { id: string; label: string; status: string; detail: string; }
export interface ToolHookToolInfo { id: string; name: string; items: ToolHookItemInfo[]; }
