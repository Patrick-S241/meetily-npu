//! Immutable OpenVINO Whisper model downloads for Windows.
use std::{collections::HashSet, path::{Path, PathBuf}, sync::Arc, time::Duration};
use dashmap::DashMap;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::{io::AsyncWriteExt, sync::{Mutex, OwnedMutexGuard}};
use super::openvino_whisper_provider::{evict_model_if_idle, helper_path_from_app, OpenVinoWhisperProvider};

const MANIFEST: &str = include_str!("openvino_models_manifest.json");
/// This closed set is required by the helper. Each artifact has a pinned SHA-256.
const MODEL_FILES: &[&str] = &[
    "added_tokens.json", "config.json", "generation_config.json", "merges.txt",
    "normalizer.json", "openvino_decoder_model.bin", "openvino_decoder_model.xml",
    "openvino_detokenizer.bin", "openvino_detokenizer.xml", "openvino_encoder_model.bin",
    "openvino_encoder_model.xml", "openvino_tokenizer.bin", "openvino_tokenizer.xml",
    "special_tokens_map.json", "tokenizer.json", "tokenizer_config.json", "vocab.json",
    "preprocessor_config.json", "openvino_config.json",
];
static MODEL_OPERATIONS: Lazy<DashMap<String, Arc<Mutex<()>>>> = Lazy::new(DashMap::new);

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest { runtime_version: String, models: Vec<Spec> }
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec { id: String, display_name: String, repo: String, revision: String, language: String, files: Vec<FileSpec> }
#[derive(Clone, Deserialize)]
struct FileSpec { path: String, size: u64, sha256: String }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo { pub id:String, pub display_name:String, pub revision:String, pub language:String, pub installed:bool, pub ready:bool, pub total_bytes:Option<u64>, pub downloaded_bytes:Option<u64>, pub reason:Option<String> }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress { pub model_id:String, pub phase:String, pub downloaded_bytes:u64, pub total_bytes:Option<u64>, pub progress:Option<u8>, pub message:Option<String> }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready { pub ready:bool, pub path:Option<String>, pub reason:Option<String> }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe { pub available:bool, pub runtime_version:Option<String>, pub device:Option<String>, pub reason:Option<String>, pub available_devices:Option<Vec<String>> }

fn catalogue() -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(MANIFEST).map_err(|e| e.to_string())?;
    for model in &manifest.models { validate_spec(model)?; }
    Ok(manifest)
}
fn spec(id: &str) -> Result<Spec, String> {
    catalogue()?.models.into_iter().find(|m| m.id == id).ok_or_else(|| format!("Unknown OpenVINO model '{id}'"))
}
fn validate_spec(model: &Spec) -> Result<(), String> {
    if model.revision.len() != 40 || !model.revision.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("OpenVINO model '{}' does not use a pinned commit revision", model.id));
    }
    let expected: HashSet<_> = MODEL_FILES.iter().copied().collect();
    let actual: HashSet<_> = model.files.iter().map(|file| file.path.as_str()).collect();
    if actual != expected || actual.len() != model.files.len() {
        return Err(format!("OpenVINO model '{}' manifest must contain exactly {} required files", model.id, MODEL_FILES.len()));
    }
    for file in &model.files {
        if file.size == 0 || file.sha256.len() != 64 || !file.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("OpenVINO model '{}' has no valid SHA-256 for {}", model.id, file.path));
        }
    }
    Ok(())
}
fn operation_lock(id: &str) -> Arc<Mutex<()>> {
    MODEL_OPERATIONS.entry(id.to_owned()).or_insert_with(|| Arc::new(Mutex::new(()))).clone()
}
pub fn cache_dir<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<PathBuf, String> {
    let manifest = catalogue()?;
    let model = manifest.models.into_iter().find(|model| model.id == id)
        .ok_or_else(|| format!("Unknown OpenVINO model '{id}'"))?;
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("models/openvino/cache").join(manifest.runtime_version).join(model.id).join(model.revision))
}
fn path<R: Runtime>(app: &AppHandle<R>, model: &Spec) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("models/openvino").join(&model.id).join(&model.revision))
}
fn bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir).ok().into_iter().flatten().filter_map(Result::ok).map(|entry| {
        if entry.path().is_dir() { bytes(&entry.path()) } else { entry.metadata().map(|m| m.len()).unwrap_or(0) }
    }).sum()
}
pub async fn ensure_ready<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<PathBuf, String> {
    let _operation = operation_lock(id).lock_owned().await;
    ensure_ready_unlocked(app, id).await
}
/// Keep the model protected from deletion until the provider has finished loading it.
pub async fn lock_and_ensure_ready<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(PathBuf, OwnedMutexGuard<()>), String> {
    let operation = operation_lock(id).lock_owned().await;
    let model_path = ensure_ready_unlocked(app, id).await?;
    Ok((model_path, operation))
}
async fn ensure_ready_unlocked<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<PathBuf, String> {
    let model = spec(id)?;
    let model_path = path(app, &model)?;
    for file in &model.files {
        let target = model_path.join(&file.path);
        let metadata = tokio::fs::metadata(&target).await.map_err(|_| format!("OpenVINO model '{}' is incomplete: missing {}", model.display_name, file.path))?;
        if metadata.len() != file.size { return Err(format!("OpenVINO model file {} has wrong size", file.path)); }
        if !sha256_file(&target).await?.eq_ignore_ascii_case(&file.sha256) {
            return Err(format!("OpenVINO model file {} failed SHA-256 validation", file.path));
        }
    }
    Ok(model_path)
}
fn emit<R: Runtime>(app: &AppHandle<R>, model: &Spec, phase: &str, done: u64, total: Option<u64>, message: Option<String>) {
    let _ = app.emit("openvino-model-download-progress", Progress {
        model_id:model.id.clone(), phase:phase.into(), downloaded_bytes:done, total_bytes:total,
        progress:total.filter(|n| *n > 0).map(|n| ((done * 100 / n).min(100)) as u8), message,
    });
}
async fn sha256_file(path: &Path) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut input = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
    let mut hash = Sha256::new(); let mut buffer = [0u8; 65_536];
    loop { let count = input.read(&mut buffer).await.map_err(|e| e.to_string())?; if count == 0 { break; } hash.update(&buffer[..count]); }
    Ok(format!("{:x}", hash.finalize()))
}

#[tauri::command]
pub async fn openvino_list_models<R: Runtime>(app: AppHandle<R>) -> Result<Vec<ModelInfo>, String> {
    let mut result = Vec::new();
    for model in catalogue()?.models {
        let model_path = path(&app, &model)?; let installed = model_path.exists();
        let checked = if installed { ensure_ready(&app, &model.id).await } else { Err("Not downloaded".into()) };
        result.push(ModelInfo { id:model.id, display_name:model.display_name, revision:model.revision, language:model.language, installed, ready:checked.is_ok(), total_bytes:None, downloaded_bytes:installed.then(||bytes(&model_path)), reason:checked.err() });
    }
    Ok(result)
}
#[tauri::command]
pub async fn openvino_download_model<R: Runtime>(app: AppHandle<R>, model_id: String) -> Result<Ready, String> {
    let _operation = operation_lock(&model_id).lock_owned().await;
    let model = spec(&model_id)?;
    if let Ok(model_path) = ensure_ready_unlocked(&app, &model_id).await {
        return Ok(Ready { ready:true, path:Some(model_path.display().to_string()), reason:None });
    }
    emit(&app, &model, "resolving", 0, None, Some(format!("OpenVINO runtime {}", catalogue()?.runtime_version)));
    let client = Client::builder().timeout(Duration::from_secs(120)).build().map_err(|e| e.to_string())?;
    let total: u64 = model.files.iter().map(|file| file.size).sum();
    let final_dir = path(&app, &model)?; let parent = final_dir.parent().ok_or("Invalid model path")?;
    tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    let stage = parent.join(format!(".{}-{}.download-{}", model.id, model.revision, uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&stage).await.map_err(|e| e.to_string())?;
    let mut done = 0;
    let result: Result<(), String> = async {
        for file in &model.files {
            let dest = stage.join(&file.path);
            for attempt in 1..=3 {
                let downloaded = async {
                    // revision is validated as a complete commit SHA, never a mutable branch or tag.
                    let url = format!("https://huggingface.co/{}/resolve/{}/{}", model.repo, model.revision, file.path);
                    let mut stream = client.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.bytes_stream();
                    let mut out = tokio::fs::File::create(&dest).await.map_err(|e| e.to_string())?; let mut got = 0;
                    while let Some(chunk) = stream.next().await { let chunk = chunk.map_err(|e| e.to_string())?; out.write_all(&chunk).await.map_err(|e| e.to_string())?; got += chunk.len() as u64; emit(&app, &model, "downloading", done + got, Some(total), Some(file.path.clone())); }
                    out.flush().await.map_err(|e| e.to_string())?;
                    if got != file.size { return Err(format!("{} size mismatch", file.path)); }
                    if !sha256_file(&dest).await?.eq_ignore_ascii_case(&file.sha256) { return Err(format!("{} SHA-256 mismatch", file.path)); }
                    Ok::<u64, String>(got)
                }.await;
                match downloaded {
                    Ok(got) => { done += got; break; }
                    Err(error) if attempt == 3 => return Err(error),
                    Err(_) => { let _ = tokio::fs::remove_file(&dest).await; tokio::time::sleep(Duration::from_millis(300)).await; }
                }
            }
        }
        for file in &model.files {
            let staged = stage.join(&file.path);
            if tokio::fs::metadata(&staged).await.map_err(|_| format!("Staged model is missing {}", file.path))?.len() != file.size || !sha256_file(&staged).await?.eq_ignore_ascii_case(&file.sha256) {
                return Err(format!("Staged model failed integrity validation for {}", file.path));
            }
        }
        let backup = parent.join(format!(".{}-{}.previous-{}", model.id, model.revision, uuid::Uuid::new_v4()));
        if final_dir.exists() { tokio::fs::rename(&final_dir, &backup).await.map_err(|e| e.to_string())?; }
        if let Err(error) = tokio::fs::rename(&stage, &final_dir).await {
            if backup.exists() { let _ = tokio::fs::rename(&backup, &final_dir).await; }
            return Err(error.to_string());
        }
        if backup.exists() { tokio::fs::remove_dir_all(&backup).await.map_err(|e| e.to_string())?; }
        Ok(())
    }.await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_dir_all(&stage).await; emit(&app, &model, "error", done, Some(total), Some(error.clone())); return Err(error);
    }
    emit(&app, &model, "complete", done, Some(total), None);
    Ok(Ready { ready:true, path:Some(final_dir.display().to_string()), reason:None })
}
#[tauri::command]
pub async fn openvino_delete_model<R: Runtime>(app: AppHandle<R>, model_id: String) -> Result<Ready, String> {
    let _operation = operation_lock(&model_id).lock_owned().await;
    let model = spec(&model_id)?; let model_path = path(&app, &model)?;
    // Provider eviction refuses while a recording, import, or retranscription owns this provider.
    evict_model_if_idle(&model_id, &model_path).await?;
    if model_path.exists() { tokio::fs::remove_dir_all(model_path).await.map_err(|e| e.to_string())?; }
    Ok(Ready { ready:false, path:None, reason:None })
}
#[tauri::command]
pub async fn openvino_probe<R: Runtime>(app: AppHandle<R>) -> Result<Probe, String> {
    let helper = helper_path_from_app(&app)?;
    match OpenVinoWhisperProvider::probe(helper).await {
        Ok(probe) => Ok(Probe { available:probe.selected_device.as_deref() == Some("NPU"), runtime_version:probe.openvino_version, device:probe.selected_device, reason:None, available_devices:Some(probe.available_devices) }),
        Err(error) => Ok(Probe { available:false, runtime_version:None, device:None, reason:Some(error.to_string()), available_devices:None }),
    }
}
#[tauri::command]
pub async fn openvino_validate_model_ready<R: Runtime>(app: AppHandle<R>, model_id: Option<String>) -> Result<Ready, String> {
    let model_id = model_id.unwrap_or_else(|| "whisper-small-int8".to_string());
    match lock_and_ensure_ready(&app, &model_id).await {
        Ok((model_path, _operation)) => match OpenVinoWhisperProvider::get_or_init(&app, &model_id, model_path, cache_dir(&app, &model_id)?).await {
            Ok(_) => Ok(Ready { ready:true, path:None, reason:None }),
            Err(error) => Ok(Ready { ready:false, path:None, reason:Some(error) }),
        },
        Err(error) => Ok(Ready { ready:false, path:None, reason:Some(error) }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_requires_all_pinned_artifacts() {
        let manifest = catalogue().expect("manifest must be immutable");
        assert_eq!(manifest.models.len(), 2);
        for model in manifest.models { assert_eq!(model.files.len(), MODEL_FILES.len()); validate_spec(&model).unwrap(); }
    }
}
