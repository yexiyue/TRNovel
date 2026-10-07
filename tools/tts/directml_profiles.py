"""Summarize native ORT provider events, not full GPU coverage or GPU kernel timing."""
import argparse
from collections import Counter
import json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("directory", type=Path)
a = p.parse_args()
rows = []
for path in sorted((a.directory / "profiles").glob("*.json")):
    providers, durations, cpu_ops, dml_ops = Counter(), Counter(), Counter(), Counter()
    for event in json.loads(path.read_text(encoding="utf-8")):
        args = event.get("args", {})
        provider = args.get("provider")
        if not provider:
            continue
        providers[provider] += 1
        durations[provider] += event.get("dur", 0)
        if provider == "CPUExecutionProvider":
            cpu_ops[args.get("op_name", "unknown")] += 1
        elif provider == "DmlExecutionProvider":
            dml_ops[args.get("op_name", "unknown")] += 1
    rows.append(dict(file=path.name, provider_events=dict(providers),
                     provider_event_duration_us=dict(durations), cpu_ops=dict(cpu_ops), dml_ops=dict(dml_ops)))
result = dict(note="Fused graph events cannot be compared with individual CPU node counts as an operator-coverage percentage. Event duration is not isolated GPU kernel time. Empty encoder profiles do not prove encoder coverage.", profiles=rows)
(a.directory / "provider-summary.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result, indent=2))
