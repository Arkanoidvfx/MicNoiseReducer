"""CPU-only import regression; uses the installed VCClient libraries, isolated files."""
import json
from pathlib import Path
import tempfile

import vcclient_server  # Installs the bundled-library loader; does not start the server.
from rvc_import import import_model
import torch
import faiss
import numpy as np


def check():
    temporary = vcclient_server.ROOT.parents[3] / ".tmp"
    with tempfile.TemporaryDirectory(prefix="rvc-import-check-", dir=temporary) as work:
        root = Path(work)
        folder = root / "model_dir"
        source = next((vcclient_server.ROOT / "model_dir" / "0").glob("*.pth"))
        slot = import_model([source], folder)
        params = json.loads((folder / str(slot) / "params.json").read_text(encoding="utf-8"))
        original = json.loads((source.parent / "params.json").read_text(encoding="utf-8"))
        for key in ("version", "sample_rate", "is_f0", "inferencer_type", "embedder"):
            assert params[key] == original[key], (key, params[key], original[key])
        # SlotManager's actual schema must accept the generated metadata.
        from vcclient.voice_changer.data_types.slot_manager_data_types import RVCSlotInfo
        RVCSlotInfo(**params)
        assert (folder / str(slot) / source.name).read_bytes() == source.read_bytes()
        model = root / "Голос тест.PTH"
        checkpoint = dict(config=[0] * 17 + [40000], version="v1", f0=0,
                          weight={"emb_g.weight": torch.zeros(1, 2),
                                  "enc_p.emb_phone.weight": torch.zeros(2, 256)})
        torch.save(checkpoint, model)
        index_file = root / "Голос.index"
        index = faiss.IndexFlatL2(256)
        index.add(np.zeros((1, 256), dtype="float32"))
        faiss.serialize_index(index).tofile(index_file)
        second = import_model([index_file, model], folder)
        assert second != slot
        metadata = json.loads((folder / str(second) / "params.json").read_text(encoding="utf-8"))
        assert metadata["inferencer_type"] == "pyTorchRVCNono"
        assert metadata["index_file"] == index_file.name
        assert (folder / str(second) / "Голос тест.pth").is_file()
        third = import_model([model], folder)
        assert third not in (slot, second)
        bad = root / "training.pth"
        torch.save({"model": {}}, bad)
        for files in ([bad], [index_file], [source, model], [source, index_file]):
            try:
                import_model(files, folder)
            except (ValueError, OSError):
                pass
            else:
                raise AssertionError(f"Invalid import accepted: {files}")
        assert len(list(folder.iterdir())) == 3
        assert not list(folder.glob(".import-*"))
    print("PASS: real RVC metadata/schema, v1/no-f0, Unicode paths/index, duplicate slots, invalid inputs, cleanup")


if __name__ == "__main__":
    check()
