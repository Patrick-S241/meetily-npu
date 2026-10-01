import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Cpu, Eye, EyeOff, House, Lock, Unlock, Zap } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { OpenVinoWhisperModelManager } from './OpenVinoWhisperModelManager';
import { usePlatform } from '@/hooks/usePlatform';
import { DEFAULT_OPENVINO_WHISPER_MODEL } from '@/constants/modelDefaults';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'openvinoWhisper' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const platform = usePlatform();
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet' || transcriptModelConfig.provider === 'openvinoWhisper') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        openvinoWhisper: [], // Model selection handled by OpenVinoWhisperModelManager component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleWhisperModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'localWhisper', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    const handleParakeetModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'parakeet', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            Transcript Model
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    setUiProvider(provider);
                                    if (provider === 'openvinoWhisper') {
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider, model: DEFAULT_OPENVINO_WHISPER_MODEL, apiKey: null });
                                    } else if (provider !== 'localWhisper' && provider !== 'parakeet') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">
                                        <span className="flex items-center gap-2"><Zap className="h-4 w-4 text-amber-500" aria-hidden="true" />Parakeet (Recommended - Real-time / Accurate)</span>
                                    </SelectItem>
                                    <SelectItem value="localWhisper">
                                        <span className="flex items-center gap-2"><House className="h-4 w-4 text-slate-600" aria-hidden="true" />Local Whisper (High Accuracy)</span>
                                    </SelectItem>
                                    {platform === 'windows' && (
                                        <SelectItem value="openvinoWhisper">
                                            <span className="flex items-center gap-2"><Cpu className="h-4 w-4 text-blue-600" aria-hidden="true" />Intel NPU (OpenVINO Whisper)</span>
                                        </SelectItem>
                                    )}
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'openvinoWhisper' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder="Select model" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[uiProvider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {uiProvider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}


                    {uiProvider === 'openvinoWhisper' && platform === 'windows' && (
                        <div className="mt-6"><OpenVinoWhisperModelManager selectedModel={transcriptModelConfig.provider === 'openvinoWhisper' ? transcriptModelConfig.model : undefined} onModelSelect={(model) => { setTranscriptModelConfig({ ...transcriptModelConfig, provider: 'openvinoWhisper', model, apiKey: null }); onModelSelect?.(); }} autoSave={true} /></div>
                    )}

                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                API Key
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => setApiKey(e.target.value)}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder="Enter your API key"
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? "Unlock to edit" : "Lock to prevent editing"}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div >
    )
}








