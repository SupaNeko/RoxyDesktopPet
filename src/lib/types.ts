export interface AppSettings {
  pet_name: string;
  persona: string;
  user_name: string;
  api_base_url: string;
  api_model: string;
  api_key_configured: boolean;
  voice_output_enabled: boolean;
  voice_output_mode: 'disabled' | 'api' | 'vits';
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
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  trigger_type: string;
  created_at: number;
}

export interface RuntimeStatus {
  database_ready: boolean;
  llm_configured: boolean;
  vits_status: 'disabled' | 'not_configured' | 'available';
  memory_status: 'not_configured' | 'unavailable' | 'available';
  microphone_status: 'disabled' | 'listening' | 'error';
  qdrant_runtime_status: string;
}

export interface VoiceStatus { state: 'disabled' | 'listening' | 'speaking' | 'recognizing' | 'thinking' | 'error'; detail?: string; duration_ms?: number; }

export interface VitsModelInfo { name: string; path: string; language: string | null; speakers: string[]; has_config: boolean; }

export interface Todo { id: string; title: string; due_at_utc: number; timezone: string; status: string; created_at: number; }
