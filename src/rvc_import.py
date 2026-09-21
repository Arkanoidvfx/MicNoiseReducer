"""Import exported RVC weights without starting the voice-conversion server."""
import ctypes
from ctypes import wintypes as wt
import importlib
import json
from pathlib import Path
import shutil
import tempfile
import zipfile


def choose_files():
    class OpenFileName(ctypes.Structure):
        _fields_ = [
            ("size", wt.DWORD), ("owner", wt.HWND), ("instance", wt.HINSTANCE),
            ("filter", wt.LPCWSTR), ("custom_filter", wt.LPWSTR),
            ("max_custom", wt.DWORD), ("filter_index", wt.DWORD),
            ("file", wt.LPWSTR), ("max_file", wt.DWORD),
            ("file_title", wt.LPWSTR), ("max_title", wt.DWORD),
            ("initial_dir", wt.LPCWSTR), ("title", wt.LPCWSTR),
            ("flags", wt.DWORD), ("file_offset", wt.WORD),
            ("extension_offset", wt.WORD), ("default_extension", wt.LPCWSTR),
            ("data", ctypes.c_ssize_t), ("hook", ctypes.c_void_p),
            ("template", wt.LPCWSTR), ("reserved", ctypes.c_void_p),
            ("reserved_size", wt.DWORD), ("flags_ex", wt.DWORD),
        ]

    buffer = ctypes.create_unicode_buffer(65536)
    dialog = OpenFileName()
    dialog.size = ctypes.sizeof(dialog)
    dialog.file = ctypes.cast(buffer, wt.LPWSTR)
    dialog.max_file = len(buffer)
    dialog.filter = "Модель RVC и индекс (*.pth; *.index)\0*.pth;*.index\0\0"
    dialog.title = "Выберите .pth и, при наличии, его .index (Ctrl + щелчок)"
    # Explorer, multiselect, existing files/path, preserve working directory.
    dialog.flags = 0x80000 | 0x200 | 0x1000 | 0x800 | 0x8
    library = ctypes.WinDLL("comdlg32", use_last_error=True)
    library.GetOpenFileNameW.argtypes = [ctypes.POINTER(OpenFileName)]
    library.GetOpenFileNameW.restype = wt.BOOL
    if not library.GetOpenFileNameW(ctypes.byref(dialog)):
        error = library.CommDlgExtendedError()
        if error:
            raise OSError(f"Окно выбора файла: ошибка {error}")
        return []
    parts = buffer[:].split("\0\0", 1)[0].split("\0")
    return [Path(parts[0])] if len(parts) == 1 else [Path(parts[0]) / p for p in parts[1:]]


def import_model(files, folder):
    models = [p for p in files if p.suffix.lower() == ".pth"]
    indexes = [p for p in files if p.suffix.lower() == ".index"]
    if len(models) != 1 or len(indexes) > 1 or len(files) != len(models) + len(indexes):
        raise ValueError("Выберите одну .pth-модель и не более одного её .index")
    source = models[0]
    if not all(p.is_file() and p.stat().st_size > 0 for p in files):
        raise ValueError("Выбранный файл отсутствует или пуст")
    if not zipfile.is_zipfile(source):
        raise ValueError("Нужен экспорт RVC .pth в современном формате PyTorch ZIP")
    torch = importlib.import_module("torch")
    try:
        checkpoint = torch.load(source, map_location="cpu", weights_only=True)
    except Exception as error:
        raise ValueError("Не удалось прочитать веса .pth; выберите экспортированную RVC-модель") from error
    if not isinstance(checkpoint, dict):
        raise ValueError("Файл не является экспортированной RVC-моделью")
    config = checkpoint.get("config")
    version = checkpoint.get("version") or "v1"
    f0 = checkpoint.get("f0")
    weights = checkpoint.get("weight")
    if (not isinstance(config, (list, tuple)) or len(config) != 18
            or version not in ("v1", "v2") or f0 not in (0, 1)
            or config[-1] not in (32000, 40000, 48000)
            or not isinstance(weights, dict) or not weights
            or not all(isinstance(t, torch.Tensor) for t in weights.values())
            or "emb_g.weight" not in weights or "enc_p.emb_phone.weight" not in weights):
        raise ValueError("Нужен экспорт RVC v1/v2; тренировочные checkpoint-файлы не поддерживаются")
    dimension = 256 if version == "v1" else 768
    if weights["enc_p.emb_phone.weight"].ndim != 2 or weights["enc_p.emb_phone.weight"].shape[1] != dimension:
        raise ValueError("Версия RVC не соответствует размеру весов модели")
    if indexes:
        faiss = importlib.import_module("faiss")
        numpy = importlib.import_module("numpy")
        try:
            index = faiss.deserialize_index(numpy.fromfile(indexes[0], dtype="uint8"))
        except Exception as error:
            raise ValueError("Не удалось прочитать .index; выберите индекс этой модели") from error
        if index.d != dimension or index.ntotal == 0:
            raise ValueError("Индекс пуст или не соответствует версии модели")
        del index
    del checkpoint, weights
    folder.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(folder).free < sum(p.stat().st_size for p in files) + 64 * 1024 * 1024:
        raise OSError("Недостаточно места для импорта модели")
    with tempfile.TemporaryDirectory(prefix=".import-", dir=folder) as temporary:
        staging = Path(temporary) / "model"
        staging.mkdir()
        model_name = source.stem + ".pth"
        index_name = indexes[0].stem + ".index" if indexes else None
        shutil.copyfile(source, staging / model_name)
        if indexes:
            shutil.copyfile(indexes[0], staging / index_name)
        params = dict(
            voice_changer_type="RVC", name=source.stem, description="", credit="",
            terms_of_use_url="", icon_file=None, speakers={}, model_file=model_name,
            index_file=index_name, is_onnx=False,
            inferencer_type=("pyTorchRVC" + ("v2" if version == "v2" else "") + ("" if f0 else "Nono")),
            sample_rate=config[-1], is_f0=bool(f0), deprecated=False,
            embedder="hubert_base_l12" if version == "v2" else "hubert_base_l9fp",
            override_embedder=None, pitch_estimator="rmvpe_onnx", sample_id=None,
            version=version, chunk_sec=0.2, pitch_shift=0, index_ratio=0.0, protect_ratio=0.5,
        )
        for slot in range(65536):
            destination = folder / str(slot)
            if destination.exists():
                continue
            params["slot_index"] = slot
            (staging / "params.json").write_text(json.dumps(params, ensure_ascii=False, indent=2), encoding="utf-8")
            try:
                staging.rename(destination)  # Windows rename never replaces an existing directory.
                return slot
            except FileExistsError:
                continue
        raise ValueError("Нет свободных слотов для модели")


def run(result, folder):
    try:
        files = choose_files()
        response = str(import_model(files, folder)) if files else "cancel"
    except Exception as error:
        response = "error: " + str(error)
    result.write_text(response, encoding="utf-8")
