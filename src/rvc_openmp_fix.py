"""Unify bundled FAISS/PyTorch OpenMP imports; run with the packaging venv."""
from pathlib import Path
import shutil
import pefile


def main():
    project = Path(__file__).resolve().parents[1]
    internal = project / "vendor/vcclient-2.1.4-alpha/dist/main/_internal"
    runtime = pefile.PE(str(internal / "torch/lib/libiomp5md.dll"))
    exports = {symbol.name for symbol in runtime.DIRECTORY_ENTRY_EXPORT.symbols}
    backup = project / ".tmp/rvc-openmp-originals"
    backup.mkdir(exist_ok=True)
    modules = list((internal / "faiss").glob("*.pyd"))
    if not modules:
        raise RuntimeError("Bundled FAISS modules not found")
    for path in modules:
        pe = pefile.PE(str(path))
        for entry in pe.DIRECTORY_ENTRY_IMPORT:
            if not entry.dll.lower().startswith(b"libomp140"):
                continue
            if any(symbol.name is None or symbol.name not in exports for symbol in entry.imports):
                raise RuntimeError(f"Incompatible OpenMP exports: {path.name}")
            original = backup / path.name
            if not original.exists():
                shutil.copy2(path, original)
            pe.set_bytes_at_rva(entry.struct.Name, b"libiomp5md.dll\0".ljust(len(entry.dll) + 1, b"\0"))
            pe.OPTIONAL_HEADER.CheckSum = pe.generate_checksum()
            staged = backup / (path.name + ".patched")
            pe.write(str(staged))
            retired = path.with_suffix(".pyd.openmp-old")
            path.rename(retired)
            try:
                shutil.copy2(staged, path)
            except BaseException:
                if not path.exists():
                    retired.rename(path)
                raise
            print(f"Updated {path.name}: OpenMP -> libiomp5md.dll")
        verified = pefile.PE(str(path))
        assert not any(e.dll.lower().startswith(b"libomp140") for e in verified.DIRECTORY_ENTRY_IMPORT)


if __name__ == "__main__":
    main()
