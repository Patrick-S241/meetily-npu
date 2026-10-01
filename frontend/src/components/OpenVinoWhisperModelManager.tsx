'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, Cpu, Download, HardDrive, Trash2 } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { DEFAULT_OPENVINO_WHISPER_MODEL } from '@/constants/modelDefaults';

interface Model {
  id: string;
  displayName?: string;
  installed?: boolean;
  ready?: boolean;
  totalBytes?: number;
  downloadedBytes?: number;
  reason?: string;
}

interface Probe {
  available: boolean;
  runtimeVersion?: string;
  device?: string;
  reason?: string;
  availableDevices?: string[];
}

interface Progress {
  modelId: string;
  phase: 'resolving' | 'downloading' | 'verifying' | 'complete' | 'error';
  downloadedBytes: number;
  totalBytes?: number;
  progress?: number;
  message?: string;
}

interface Props {
  selectedModel?: string;
  onModelSelect?: (model: string) => void;
  className?: string;
  autoSave?: boolean;
}

const fallbackModelNames: Record<string, string> = {
  'whisper-base-int8': 'Whisper Base INT8',
  'whisper-small-int8': 'Whisper Small INT8',
  'whisper-medium-int8': 'Whisper Medium INT8',
  'whisper-large-v3-turbo-int4': 'Whisper Large V3 Turbo INT4',
};

function formatBytes(bytes?: number): string | null {
  if (bytes === undefined) return null;
  if (bytes === 0) return '0 Bytes';
  const units = ['Bytes', 'KB', 'MB', 'GB'];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** unit).toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function OpenVinoWhisperModelManager({ selectedModel, onModelSelect, className = '', autoSave = false }: Props) {
  const [probe, setProbe] = useState<Probe | null>(null);
  const [isProbing, setIsProbing] = useState(true);
  const [models, setModels] = useState<Model[]>([]);
  const [events, setEvents] = useState<Record<string, Progress>>({});
  const [error, setError] = useState<string | null>(null);
  const [selectingModelId, setSelectingModelId] = useState<string | null>(null);
  const [selectionElapsedSeconds, setSelectionElapsedSeconds] = useState(0);
  const mounted = useRef(true);
  const refreshId = useRef(0);
  const selectInFlight = useRef<string | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const refresh = useCallback(async () => {
    if (!mounted.current) return;
    const id = ++refreshId.current;
    setIsProbing(true);
    setError(null);
    const [probeResult, modelsResult] = await Promise.allSettled([
      invoke<Probe>('openvino_probe'),
      invoke<Model[]>('openvino_list_models'),
    ]);
    if (!mounted.current || id !== refreshId.current) return;
    setProbe(probeResult.status === 'fulfilled' ? probeResult.value : { available: false, reason: String(probeResult.reason) });
    if (modelsResult.status === 'fulfilled') setModels(modelsResult.value);
    else setError(String(modelsResult.reason));
    setIsProbing(false);
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    let off: (() => void) | undefined;
    listen<Progress>('openvino-model-download-progress', (event) => {
      setEvents((current) => ({ ...current, [event.payload.modelId]: event.payload }));
      if (event.payload.phase === 'complete' || event.payload.phase === 'error') void refresh();
    }).then((unlisten) => { off = unlisten; }).catch(console.error);
    return () => off?.();
  }, [refresh]);

  useEffect(() => {
    if (!selectingModelId) return;
    const started = Date.now();
    setSelectionElapsedSeconds(0);
    const timer = window.setInterval(() => setSelectionElapsedSeconds(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [selectingModelId]);

  const select = async (id: string) => {
    if (selectInFlight.current) return;
    selectInFlight.current = id;
    if (mounted.current) setSelectingModelId(id);
    try {
      const result = await invoke<{ ready: boolean; reason?: string }>('openvino_validate_model_ready', { modelId: id });
      if (!result.ready) throw new Error(result.reason ?? 'Model validation failed');
      onModelSelect?.(id);
      if (autoSave) await invoke('api_save_transcript_config', { provider: 'openvinoWhisper', model: id, apiKey: null });
      void refresh();
    } catch (selectError) {
      toast.error('OpenVINO model is not ready', { description: String(selectError) });
    } finally {
      selectInFlight.current = null;
      if (mounted.current) setSelectingModelId(null);
    }
  };

  const download = async (id: string) => {
    try { await invoke('openvino_download_model', { modelId: id }); }
    catch (downloadError) { toast.error('Could not download OpenVINO model', { description: String(downloadError) }); }
  };

  const remove = async (id: string) => {
    try { await invoke('openvino_delete_model', { modelId: id }); void refresh(); }
    catch (deleteError) { toast.error('Could not delete OpenVINO model', { description: String(deleteError) }); }
  };

  const all: Model[] = models.length
    ? models
    : Object.entries(fallbackModelNames).map(([id, displayName]) => ({ id, displayName }));
  const available = probe?.available === true;

  return (
    <section className={`space-y-3 ${className}`}>
      <div className="rounded-lg border border-blue-200 bg-blue-50 p-4 text-sm text-blue-950">
        <div className="flex items-center gap-2 font-medium"><Cpu className="h-5 w-5" aria-hidden="true" /> Intel NPU (OpenVINO Whisper)</div>
        <p className="mt-1">Whisper inference runs on the Intel NPU. GenAI uses the CPU for tokenization; there is no CPU/GPU fallback for Whisper inference.</p>
      </div>

      {isProbing ? <div className="rounded-lg border border-blue-200 bg-blue-50 p-3 text-sm text-blue-900" role="status"><p className="font-medium">Checking Intel NPU availability...</p></div>
        : available ? <div className="rounded-lg border border-green-200 bg-green-50 p-3 text-sm text-green-900"><p className="font-medium">NPU detected{probe?.device ? `: ${probe.device}` : ''}</p><p>OpenVINO runtime: {probe?.runtimeVersion ?? 'reported by backend'}</p></div>
          : <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900"><p className="font-medium">OpenVINO Whisper unavailable</p><p>{probe?.reason ?? 'OpenVINO did not report a supported Intel NPU.'}</p>{probe?.availableDevices?.length ? <p>Detected OpenVINO devices: {probe.availableDevices.join(', ')}</p> : null}<button disabled={selectingModelId !== null} className="mt-2 underline disabled:text-gray-500" onClick={() => void refresh()}>Retry probe</button></div>}

      {error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">Could not load OpenVINO model status: {error} <button disabled={selectingModelId !== null} className="underline disabled:text-gray-500" onClick={() => void refresh()}>Retry</button></div> : null}

      {all.map((model) => {
        const event = events[model.id];
        const state = event?.phase === 'error' ? 'failed' : event?.phase === 'verifying' ? 'validating' : event?.phase === 'downloading' || event?.phase === 'resolving' ? 'downloading' : model.ready ? 'ready' : model.installed ? 'downloaded' : 'not downloaded';
        const isSelected = selectedModel === model.id;
        const isSelecting = selectingModelId === model.id;
        const isBusy = selectingModelId !== null;
        const totalBytes = event?.totalBytes ?? model.totalBytes;
        const downloadedBytes = event?.downloadedBytes ?? model.downloadedBytes;
        const percentage = event?.progress ?? (downloadedBytes !== undefined && totalBytes ? downloadedBytes / totalBytes * 100 : 0);
        const modelSize = formatBytes(totalBytes);
        const progressSize = downloadedBytes === undefined ? null : `${formatBytes(downloadedBytes)}${modelSize ? ` / ${modelSize}` : ''}`;
        const modelName = model.displayName ?? fallbackModelNames[model.id] ?? model.id;
        const status = isSelecting
          ? { label: 'Preparing NPU', dotClassName: 'bg-blue-500', textClassName: 'text-blue-700' }
          : state === 'ready'
          ? { label: 'Installed', dotClassName: 'bg-green-500', textClassName: 'text-green-600' }
          : state === 'downloading' || state === 'validating'
            ? { label: state === 'validating' ? 'Validating' : 'Downloading', dotClassName: 'bg-blue-500', textClassName: 'text-blue-700' }
            : state === 'failed'
              ? { label: 'Download failed', dotClassName: 'bg-red-500', textClassName: 'text-red-700' }
              : state === 'downloaded'
                ? { label: 'Needs validation', dotClassName: 'bg-amber-500', textClassName: 'text-amber-700' }
                : { label: 'Not downloaded', dotClassName: 'bg-gray-400', textClassName: 'text-gray-600' };

        return <div key={model.id} className={`relative rounded-lg border-2 p-4 transition-colors ${isSelected ? 'border-blue-500 bg-blue-50' : state === 'ready' ? 'border-gray-200 bg-white hover:border-gray-300' : 'border-gray-200 bg-gray-50'}`}>
          {model.id === DEFAULT_OPENVINO_WHISPER_MODEL ? <span className="absolute -right-2 -top-2 rounded-full bg-blue-600 px-2 py-0.5 text-xs font-medium text-white">Recommended</span> : null}
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <Cpu className="h-5 w-5 text-blue-600" aria-hidden="true" />
                <p className="font-semibold text-gray-900">{modelName}</p>
                {isSelected ? <span className="flex items-center gap-1 rounded-full bg-blue-600 px-2 py-0.5 text-xs font-medium text-white"><Check className="h-3 w-3" aria-hidden="true" /> Selected</span> : null}
              </div>
              <p className="ml-7 mt-1 text-sm text-gray-600">Multilingual Whisper model for local Intel NPU transcription.</p>
              {modelSize ? <p className="ml-7 mt-2 flex items-center gap-1.5 text-sm text-gray-600"><HardDrive className="h-4 w-4" aria-hidden="true" />{modelSize}</p> : null}
              <p className="mt-1 text-xs text-gray-500">{isSelecting ? `Preparing model on the NPU… ${selectionElapsedSeconds}s elapsed. The first compilation can take several minutes.` : event?.message ?? (state === 'ready' ? 'Model files verified. NPU loading is checked on selection.' : state)}</p>
              {model.reason ? <p className="text-xs text-red-700">{model.reason}</p> : null}
            </div>

            <div className="flex shrink-0 flex-col items-end gap-2">
              <div className={`flex items-center gap-1.5 text-xs font-medium ${status.textClassName}`}>
                <span className={`h-2 w-2 rounded-full ${status.dotClassName}`} aria-hidden="true" />
                {status.label}
              </div>
              {state === 'ready' ? <>
                <button disabled={isBusy} className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:bg-gray-400" onClick={() => void select(model.id)}>{isSelecting ? 'Preparing…' : isSelected ? 'Check NPU' : 'Select'}</button>
                <button disabled={isBusy} className="flex items-center gap-1 rounded border px-3 py-1.5 text-sm disabled:text-gray-400" onClick={() => void remove(model.id)}><Trash2 className="h-4 w-4" aria-hidden="true" />Delete</button>
              </> : state === 'downloaded' ? <>
                <button disabled={!available || isBusy} className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:bg-gray-400" onClick={() => void select(model.id)}>{isSelecting ? 'Preparing…' : 'Validate'}</button>
                <button disabled={isBusy} className="flex items-center gap-1 rounded border px-3 py-1.5 text-sm disabled:text-gray-400" onClick={() => void remove(model.id)}><Trash2 className="h-4 w-4" aria-hidden="true" />Delete</button>
              </> : state === 'downloading' || state === 'validating' ? <span className="text-sm text-blue-700">{state === 'downloading' ? `${Math.round(percentage)}%` : 'Preparing...'}</span>
                : <button disabled={!available || isBusy} className="flex items-center gap-1 rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:bg-gray-400" onClick={() => void download(model.id)}><Download className="h-4 w-4" aria-hidden="true" />{state === 'failed' ? 'Retry' : 'Download'}</button>}
            </div>
          </div>

          {state === 'downloading' ? <div className="mt-3 border-t border-gray-200 pt-3"><div className="mb-2 flex items-center justify-between text-xs text-gray-600"><span>Downloading...</span><span>{progressSize ?? `${Math.round(percentage)}%`}</span></div><div className="h-2 overflow-hidden rounded bg-gray-200"><div className="h-full bg-blue-600" style={{ width: `${Math.max(0, Math.min(100, percentage))}%` }} /></div></div> : null}
        </div>;
      })}
    </section>
  );
}
