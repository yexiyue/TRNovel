"""Sequential Mac acceptance/benchmark runner; retains failed cases as evidence."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--probe", required=True)
p.add_argument("--model-root", required=True)
p.add_argument("--coreml-root", help="Optional separate cache root for cold/cache CoreML measurements")
p.add_argument("--omnivoice-coreml-root", help="Optional OmniVoice cache root after the encoder fallback fix")
p.add_argument("--output", required=True, type=Path)
p.add_argument("--phase", choices=("voices", "benchmark", "quality"), required=True)
p.add_argument("--clone", default="custom:onnx-acceptance")
p.add_argument("--resume", action="store_true", help="Retain successful cases and rerun incomplete/failed cases")
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
short = "你好，欢迎使用听书功能。"
corpora = {
    "short": short,
    "paragraph": "山风吹过松林，远处的灯火渐渐亮起。他停下脚步，轻声说：我们终于到家了。",
    "long": "清晨的阳光穿过窗帘，落在桌上的旧书旁。她翻开书页，读到了熟悉的名字。山风吹过松林，远处的灯火渐渐亮起。他停下脚步，轻声说：我们终于到家了。夜色安静下来，窗外只剩下雨声。他想起一路走过的村庄，也想起那些还没有说出口的话。",
}
voices = ("vivian", "serena", "uncle_fu", "dylan", "eric", "ryan", "aiden", "ono_anna", "sohee")
paths = [("qwen-onnx", "0.6b-customvoice", "cpu", "uncle_fu"),
         ("qwen-onnx", "1.7b-customvoice", "cpu", "uncle_fu"),
         ("omnivoice-onnx", "0.6b", "cpu", "narrator")]
jobs = []
if a.phase == "voices":
    for backend, model, device, _ in paths[:2]:
        jobs.append((f"{model}-nine-voices", backend, model, device, "uncle_fu", short, None,
                     {"NOVEL_TTS_PROBE_VOICES": ",".join(voices)}))
    for style in ("自然平静地讲述", "非常开心，充满喜悦地讲述", "悲伤低落，带着难过的情绪讲述"):
        for backend, device in (("qwen-onnx", "cpu"), ("qwen", "metal")):
            jobs.append((f"{backend}-style-{len(jobs)}", backend, "1.7b-customvoice", device,
                         "uncle_fu", "我们终于到家了，今天的故事就讲到这里。", style, {}))
    for voice in ("narrator", a.clone):
        for backend, device in (("omnivoice-onnx", "cpu"), ("omnivoice", "metal")):
            jobs.append((f"{backend}-{voice.replace(':', '-')}", backend, "0.6b", device, voice, short, None, {}))
    for backend, model, device, voice in paths:
        jobs.append((f"{backend}-{model}-cancel", backend, model, device, voice, short, None,
                     {"NOVEL_TTS_PROBE_CANCEL": "1", "NOVEL_TTS_PROBE_DROP_EARLY": "1"}))
elif a.phase == "quality":
    for style in ("自然平静地讲述", "非常开心，充满喜悦地讲述", "悲伤低落，带着难过的情绪讲述"):
        for backend in ("qwen-onnx", "qwen"):
            flags = {"NOVEL_TTS_QWEN_GREEDY_CONTROL": "1"} if backend == "qwen-onnx" else {}
            jobs.append((f"{backend}-fp32-style-{len(jobs)}", backend, "1.7b-customvoice", "cpu",
                         "uncle_fu", "我们终于到家了，今天的故事就讲到这里。", style, flags))
    for voice in ("narrator", a.clone):
        for backend in ("omnivoice-onnx", "omnivoice"):
            jobs.append((f"{backend}-fp32-{voice.replace(':', '-')}", backend, "0.6b", "cpu", voice, short, None, {}))
        jobs.append((f"omnivoice-onnx-mid-cancel-{voice.replace(':', '-')}", "omnivoice-onnx", "0.6b", "cpu", voice, short, None,
                     {"NOVEL_TTS_PROBE_CANCEL_EARLY": "1", "NOVEL_TTS_PROBE_DROP_EARLY": "1"}))
else:
    paths += [(b, m, "coreml", v) for b, m, _, v in paths.copy()]
    paths += [("qwen", "0.6b-customvoice", "metal", "uncle_fu"),
              ("qwen", "1.7b-customvoice", "metal", "uncle_fu"),
              ("omnivoice", "0.6b", "metal", "narrator")]
    for backend, model, device, voice in paths:
        for corpus, text in corpora.items():
            jobs.append((f"{backend}-{model}-{device}-{corpus}", backend, model, device, voice, text, None,
                         {"NOVEL_TTS_PROBE_RUNS": "4"}))
    for model in ("0.6b-customvoice", "1.7b-customvoice"):
        jobs.append((f"qwen-onnx-{model}-cpu-greedy-control", "qwen-onnx", model, "cpu", "uncle_fu", short, None,
                     {"NOVEL_TTS_PROBE_RUNS": "4", "NOVEL_TTS_QWEN_GREEDY_CONTROL": "1"}))
        jobs.append((f"qwen-{model}-cpu-fp32-control", "qwen", model, "cpu", "uncle_fu", short, None,
                     {"NOVEL_TTS_PROBE_RUNS": "4"}))

result_path = a.output / "results.json"
results = [item for item in json.loads(result_path.read_text()) if item["exit_code"] == 0] if a.resume and result_path.exists() else []
completed = {item["name"] for item in results}
for name, backend, model, device, voice, text, style, flags in jobs:
    if name in completed:
        continue
    log_path = a.output / f"{name}.log"
    model_root = a.coreml_root if device == "coreml" and a.coreml_root else a.model_root
    if backend == "omnivoice-onnx" and device == "coreml" and a.omnivoice_coreml_root:
        model_root = a.omnivoice_coreml_root
    command = ["/usr/bin/time", "-l", a.probe, backend, model, model_root,
               str(a.output / f"{name}.wav"), device, text, voice]
    if style:
        command.append(style)
    env = dict(os.environ, **flags)
    print(f"START {name}", flush=True)
    try:
        with log_path.open("w") as log:
            run = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=1800)
        rows = []
        peak_rss = None
        peak_footprint = None
        for line in log_path.read_text().splitlines():
            if line.startswith("{"):
                rows.append(json.loads(line))
            if "maximum resident set size" in line:
                peak_rss = int(line.split()[0])
            if "peak memory footprint" in line:
                peak_footprint = int(line.split()[0])
        measured = [row for row in rows if "rtf" in row and not row.get("warmup", False)]
        item = dict(name=name, backend=backend, model=model, device=device, voice=voice, style=style,
                    text=text, exit_code=run.returncode, peak_rss_bytes=peak_rss,
                    peak_footprint_bytes=peak_footprint, runs=rows)
        if measured:
            item["median"] = {key: statistics.median(row[key] for row in measured)
                              for key in ("rtf", "first_pcm_ms", "generate_ms", "audio_seconds", "load_ms")}
    except subprocess.TimeoutExpired:
        item = dict(name=name, exit_code="timeout", log=str(log_path))
    results.append(item)
    (a.output / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n")
    print(f"END {name}: {item['exit_code']} {item.get('median', {})}", flush=True)
if any(item["exit_code"] != 0 for item in results):
    raise SystemExit(1)
