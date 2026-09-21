import importlib.abc
import importlib.util
import os
import sys
import traceback
import types
import argparse
from pathlib import Path

from PyInstaller.archive.readers import CArchiveReader


ROOT = (
    Path(sys.executable).resolve().parent
    if getattr(sys, "frozen", False)
    else Path(__file__).resolve().parents[1]
    / "vendor"
    / "vcclient-2.1.4-alpha"
    / "dist"
    / "main"
)
INTERNAL = ROOT / "_internal"
ERROR_LOG = ROOT.parents[3] / "results" / "rvc-server-error.log"
REQUIRED = {
    ROOT / "main.exe": None,
    ROOT / "modules" / "contentvec" / "hubert_base.pt": 189_507_909,
    ROOT / "modules" / "contentvec" / "contentvec-f.onnx": 378_550_151,
    ROOT / "modules" / "rmvpe" / "rmvpe_20231006.onnx": 362_003_174,
}


def log_uncaught(kind, value, tb):
    ERROR_LOG.parent.mkdir(parents=True, exist_ok=True)
    with ERROR_LOG.open("w", encoding="utf-8") as log:
        traceback.print_exception(kind, value, tb, file=log)


sys.excepthook = log_uncaught


def check_runtime():
    for path, size in REQUIRED.items():
        if not path.is_file() or (size is not None and path.stat().st_size != size):
            raise RuntimeError(f"Missing or invalid VCClient file: {path}")


check_runtime()
PYZ = CArchiveReader(str(ROOT / "main.exe")).open_embedded_archive("PYZ-00.pyz")


class PyzFinder(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    def find_spec(self, fullname, path=None, target=None):
        entry = PYZ.toc.get(fullname)
        if entry is None:
            return None
        is_package = entry[0] in (1, 3)
        spec = importlib.util.spec_from_loader(fullname, self, is_package=is_package)
        if is_package:
            package_dir = INTERNAL.joinpath(*fullname.split("."))
            if package_dir.is_dir():
                spec.submodule_search_locations.append(str(package_dir))
        return spec

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        code = PYZ.extract(module.__name__)
        if code is None:
            return
        module_path = INTERNAL.joinpath(*module.__name__.split("."))
        module.__file__ = str(
            module_path / "__init__.pyc"
            if module.__spec__.submodule_search_locations is not None
            else module_path.with_suffix(".pyc")
        )
        exec(code, module.__dict__)


os.chdir(ROOT)
sys._MEIPASS = str(INTERNAL)
sys.path.insert(0, str(INTERNAL))
DLL_HANDLES = []
for relative in (
    "",
    "numpy.libs",
    "scipy.libs",
    "torch/lib",
    "torch/bin",
    "onnxruntime/capi",
    "faiss_cpu.libs",
    "_soundfile_data",
    "_sounddevice_data/portaudio-binaries",
):
    directory = INTERNAL / relative
    if directory.is_dir():
        DLL_HANDLES.append(os.add_dll_directory(directory))
sys.meta_path.insert(0, PyzFinder())

certifi = types.ModuleType("certifi")
certifi.where = lambda: str(INTERNAL / "certifi" / "cacert.pem")
certifi.contents = lambda: Path(certifi.where()).read_text(encoding="ascii")
sys.modules["certifi"] = certifi

# VCClient's bundled PortAudio crashes on this machine. MicNoiseReducer owns all
# devices, so its REST-only sidecar intentionally exposes no local audio devices.
sounddevice = types.ModuleType("sounddevice")
sounddevice.PortAudioError = type("PortAudioError", (Exception,), {})
sounddevice._terminate = lambda: None
sounddevice._initialize = lambda: None
sounddevice.query_devices = lambda *args, **kwargs: []
sounddevice.query_hostapis = lambda *args, **kwargs: []
sys.modules["sounddevice"] = sounddevice
sys.modules["_sounddevice_data"] = types.ModuleType("_sounddevice_data")


def main():
    import numpy as np
    import uvicorn
    from fastapi import FastAPI, HTTPException, Request, Response
    from vcclient.voice_changer.auido_device_manager.audio_device_manager import (
        AudioDeviceManager,
    )
    from vcclient.voice_changer.configuration_manager.configuration_manager import (
        ConfigurationManager,
    )
    from vcclient.voice_changer.gpu_device_manager.gpu_device_manager import (
        GPUDeviceManager,
    )
    from vcclient.voice_changer.module_manager.module_manager import ModuleManager
    from vcclient.voice_changer.slot_manager.slot_manager import SlotManager
    from vcclient.voice_changer.voice_change_manager.voice_changer import VoiceChanger

    AudioDeviceManager.get_instance().reload_device()
    ConfigurationManager.get_instance().reload()
    GPUDeviceManager.get_instance().reload()
    ModuleManager.get_instance().reload()
    SlotManager.get_instance().reload()
    voice_changer = VoiceChanger.get_instance()
    voice_changer.initialize()
    parser = argparse.ArgumentParser()
    parser.add_argument("--slot", type=int, default=0)
    parser.add_argument("--chunk-ms", type=int, default=200)
    parser.add_argument("--pitch", type=int, default=0)
    parser.add_argument("--index", type=int, default=0)
    args = parser.parse_args()
    current = None

    def configure(slot, pitch, index, chunk_ms):
        nonlocal current
        if not (0 <= slot <= 65535 and -24 <= pitch <= 24 and
                0 <= index <= 100 and chunk_ms in (100, 150, 200, 300, 500)):
            raise HTTPException(400, "Invalid RVC settings")
        wanted = (slot, pitch, index, chunk_ms)
        if wanted == current:
            return
        slots = SlotManager.get_instance()
        if not (ROOT / "model_dir" / str(slot) / "params.json").is_file():
            raise HTTPException(400, "Model slot not found")
        if current is None or current[0] != slot:
            slots.reload(False)
        try:
            info = slots.get_slot_info(slot)
        except Exception as exc:
            raise HTTPException(400, "Model slot not found") from exc
        if info.voice_changer_type != "RVC":
            raise HTTPException(400, "Select an RVC model")
        info.pitch_shift = pitch
        info.index_ratio = index / 100 if info.index_file else 0.0
        info.chunk_sec = chunk_ms / 1000
        conf = ConfigurationManager.get_instance().get_voice_changer_configuration()
        conf.current_slot_index = slot
        conf.pass_through = False
        conf.recording_started = False
        conf.input_sample_rate = conf.output_sample_rate = 48000
        if current is None or current[0] != slot:
            # Release the old pipeline before allocating another model on the GPU.
            voice_changer.vc_pipeline = None
            voice_changer.initialize()
        voice_changer.check_and_update_pipeline()
        voice_changer.vc_pipeline.slot_info = info
        voice_changer.vc_chunk_sec = info.chunk_sec
        voice_changer.chunk_size = round(48000 * info.chunk_sec)
        current = wanted

    configure(args.slot, args.pitch, args.index, args.chunk_ms)
    # Warm the actual conversion path before advertising readiness.
    warm = (0.01 * np.sin(np.arange(voice_changer.chunk_size) * 0.03)).astype(np.float32)
    for _ in range(2):
        voice_changer.convert_chunk(warm)

    app = FastAPI()
    last_stream = None

    @app.get("/mnr/ready")
    def ready():
        return dict(zip(("slot", "pitch", "index", "chunk_ms"), current))

    @app.post("/mnr/convert")
    async def convert(request: Request, slot: int = 0, pitch: int = 0,
                      index: int = 0, chunk_ms: int = 200, stream: int = 0):
        nonlocal last_stream
        if request.headers.get("content-type") != "application/octet-stream":
            raise HTTPException(415, "Expected float32 PCM")
        # Bound the body before parsing; the audio worker is the only client.
        body = bytearray()
        async for part in request.stream():
            body.extend(part)
            if len(body) > 48000 * 4 // 2:
                raise HTTPException(413, "Audio block too large")
        if chunk_ms not in (100, 150, 200, 300, 500) or len(body) != chunk_ms * 48 * 4:
            raise HTTPException(400, "Incorrect audio block length")
        audio = np.frombuffer(body, dtype=np.float32)
        if not np.isfinite(audio).all():
            raise HTTPException(400, "Non-finite audio")
        try:
            configure(slot, pitch, index, chunk_ms)
            if stream != last_stream:
                voice_changer.crossfade_processor = type(voice_changer.crossfade_processor)()
                for name in ("audio_buffer", "pitchf_buffer", "feature_buffer"):
                    getattr(voice_changer.vc_pipeline, name).fill(0)
                voice_changer.vc_pipeline.prev_vol = 0.0
                last_stream = stream
            converted, _, _ = voice_changer.convert_chunk(audio)
            converted = np.asarray(converted, dtype=np.float32)
            if not np.isfinite(converted).all() or not 0 < len(converted) <= len(audio):
                raise RuntimeError("Invalid converted audio")
            # First SOLA block may be shorter. Preserve duration without a second queue.
            output = np.zeros(len(audio), dtype=np.float32)
            output[-len(converted):] = np.clip(converted, -1, 1)
            return Response(output.tobytes(), media_type="application/octet-stream")
        except HTTPException:
            raise
        except Exception:
            log_uncaught(*sys.exc_info())
            raise HTTPException(500, "RVC conversion failed; see results/rvc-server-error.log")

    uvicorn.run(app, host="127.0.0.1", port=18889, log_config=None, access_log=False)


if __name__ == "__main__":
    try:
        if len(sys.argv) == 3 and sys.argv[1] == "--import-model":
            from rvc_import import run
            run(Path(sys.argv[2]), ROOT / "model_dir")
        else:
            main()
    except BaseException:
        traceback.print_exc()
        raise
