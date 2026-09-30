export interface ModelWithStatus { status?: unknown; }
interface ProviderCommands { initialize: string; hasAvailableModels: string; getAvailableModels: string; }
const PROVIDER_COMMANDS: Record<string, ProviderCommands> = {
  localWhisper: { initialize: 'whisper_init', hasAvailableModels: 'whisper_has_available_models', getAvailableModels: 'whisper_get_available_models' },
  parakeet: { initialize: 'parakeet_init', hasAvailableModels: 'parakeet_has_available_models', getAvailableModels: 'parakeet_get_available_models' },
  openvinoWhisper: { initialize: 'openvino_probe', hasAvailableModels: 'openvino_validate_model_ready', getAvailableModels: 'openvino_list_models' },
};
export function getProviderCommands(provider: string): ProviderCommands | null { return PROVIDER_COMMANDS[provider] ?? null; }
export function hasDownloadingModel(models: ModelWithStatus[]): boolean { return models.some(({ status }) => status === 'Downloading' || status === 'downloading' || (status !== null && typeof status === 'object' && 'Downloading' in status)); }
