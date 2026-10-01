"""Pin OpenVINO Whisper download metadata from immutable Hugging Face revisions.

Developer-only utility. Run from the repository root when adding catalog models.
Large LFS files use the hub's SHA-256 metadata; smaller regular files are
downloaded and hashed. The application never calls this script.
"""

import hashlib
import json
import pathlib
import urllib.parse
import urllib.request


MANIFEST = pathlib.Path("frontend/src-tauri/src/audio/transcription/openvino_models_manifest.json")
NEW_MODELS = (
    (
        "whisper-medium-int8",
        "Whisper Medium INT8",
        "OpenVINO/whisper-medium-int8-ov",
        "8d43cce846729381f56bd45a1c70925cee2222ff",
    ),
    (
        "whisper-large-v3-turbo-int4",
        "Whisper Large V3 Turbo INT4",
        "OpenVINO/whisper-large-v3-turbo-int4-ov",
        "ae50b4d9a9dbaf16f2df59c23f3984e42f864dfc",
    ),
)


def get(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "Meetily-manifest-pinner"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def main() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    required = [file["path"] for file in manifest["models"][0]["files"]]
    assert len(required) == len(set(required))
    for model_id, display_name, repo, revision in NEW_MODELS:
        if any(model["id"] == model_id for model in manifest["models"]):
            continue
        api = f"https://huggingface.co/api/models/{repo}/revision/{revision}?blobs=true"
        metadata = json.loads(get(api))
        if metadata["sha"] != revision:
            raise ValueError(f"Unexpected revision for {repo}: {metadata['sha']}")
        siblings = {item["rfilename"]: item for item in metadata["siblings"]}
        if not set(required).issubset(siblings):
            raise ValueError(f"Missing model artifacts for {repo}: {set(required) - siblings.keys()}")
        files = []
        for name in required:
            item = siblings[name]
            size = item["size"]
            lfs = item.get("lfs")
            if lfs:
                if size != lfs["size"]:
                    raise ValueError(f"LFS size mismatch for {repo}/{name}")
                sha256 = lfs["sha256"]
            else:
                if size > 16 * 1024 * 1024:
                    raise ValueError(f"Large artifact lacks LFS hash: {repo}/{name}")
                url = f"https://huggingface.co/{repo}/resolve/{revision}/{urllib.parse.quote(name)}"
                content = get(url)
                if len(content) != size:
                    raise ValueError(f"Downloaded size mismatch for {repo}/{name}")
                sha256 = hashlib.sha256(content).hexdigest()
            files.append({"path": name, "size": size, "sha256": sha256})
        manifest["models"].append(
            {
                "id": model_id,
                "revision": revision,
                "files": files,
                "repo": repo,
                "language": "multilingual",
                "displayName": display_name,
            }
        )
        print(f"Pinned {model_id}: {sum(file['size'] for file in files):,} bytes")
    MANIFEST.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
