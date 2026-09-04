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
  tool_hook_tool_texts: Record<string, ToolHookToolText>;
  tool_hook_include_last_message: boolean;
  tool_hook_min_interval_minutes: number;
  tool_hook_daily_limit: number;
  tool_hook_debounce_seconds: number;
  tool_hook_voice_enabled: boolean;
  system_status_enabled: boolean;
  taskbar_apps_enabled: boolean;
  now_playing_enabled: boolean;
  voice_input_mode: 'disabled' | 'continuous' | 'push_to_talk';
  push_to_talk_shortcut: string;
  pet_show_on_fullscreen: boolean;
  pet_outfit: string;
  search_provider: '' | 'bocha' | 'tavily';
  search_api_key: string;
  search_base_url: string;
  search_api_configured: boolean;
  agent_max_tool_rounds: number;
  autostart_enabled: boolean;
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
  voice_hardware_status: 'supported' | 'unsupported';
  voice_hardware_detail: string;
  voice_gpu: string | null;
}

export interface VoiceStatus { state: 'disabled' | 'listening' | 'speaking' | 'recognizing' | 'thinking' | 'error'; detail?: string; duration_ms?: number; }

export interface VitsModelInfo { name: string; path: string; language: string | null; speakers: string[]; has_config: boolean; }

export interface Todo { id: string; title: string; due_at_utc: number; timezone: string; status: string; repeat_interval_minutes: number | null; created_at: number; }

export interface ToolHookStatus { status: 'not_configured' | 'configured' | 'needs_update' | 'error'; detail: string; }
export interface ToolHookItemInfo { id: string; label: string; status: string; detail: string; }
export interface ToolHookToolText { fixed_text: string; fixed_voice_text: string; }
export interface ToolHookToolInfo { id: string; name: string; default_fixed_text: string; default_fixed_voice_text: string; items: ToolHookItemInfo[]; }

export interface TaskbarApp { title: string; process_name: string; pid: number; foreground: boolean; }

export type McpTransport = 'stdio' | 'remote';
export type McpConnectionStatus = 'disabled' | 'connected' | 'pending_restart' | 'error';

export interface McpServer {
  id: number;
  name: string;
  transport: McpTransport;
  command: string;
  args: string;
  env: string;
  url: string;
  headers: string;
  timeout_seconds: number;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface McpServerStatus extends McpServer {
  status: McpConnectionStatus;
  tool_count: number;
  requires_restart: boolean;
  error: string | null;
}

export interface McpServerRequest {
  name: string;
  transport: McpTransport;
  command: string;
  args: string;
  env: string;
  url: string;
  headers: string;
  /** 单次工具调用超时（秒） */
  timeout_seconds: number;
  /** 非空时后端忽略其余字段，直接按 JSON 配置解析 */
  config_json?: string;
}

export interface McpToolInfo { name: string; description: string; }

export interface HardwareStats {
  cpu_usage_percent: number;
  memory_used_mb: number;
  memory_total_mb: number;
  gpu_name: string | null;
  gpu_usage_percent: number | null;
  gpu_memory_used_mb: number | null;
  gpu_memory_total_mb: number | null;
  gpu_temperature_celsius: number | null;
}

export interface NowPlaying { title: string; artist: string; album: string; source_app: string; playing: boolean; }

/** 单个分类的 API 消耗汇总（今日 / 累计两段） */
export interface UsageSummary {
  category: string;
  calls_today: number;
  prompt_today: number;
  completion_today: number;
  calls_total: number;
  prompt_total: number;
  completion_total: number;
}
