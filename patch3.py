import json
import os

manifest_path = ".agent/manifest.json"
if os.path.exists(manifest_path):
    with open(manifest_path, "r") as f:
        m = json.load(f)
    m["version"] = "0.5.0"
    with open(manifest_path, "w") as f:
        json.dump(m, f, indent=2)

phase1 = ".agent/workflows/phase1-timers-messages-errors.md"
if os.path.exists(phase1):
    os.remove(phase1)

print("Phase 3 done")
