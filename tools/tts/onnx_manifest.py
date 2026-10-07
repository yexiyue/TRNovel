"""Content hashes and provenance for development-only ONNX exports."""
import hashlib
import json
import pathlib
import subprocess
import onnx


def require_revision(directory, expected):
    actual = subprocess.check_output(
        ["git", "-C", str(directory), "rev-parse", "HEAD"], text=True
    ).strip()
    if actual != expected:
        raise ValueError(f"Expected export source {expected}, got {actual}")
    return actual


def write_manifest(output, revision, source_revision, parameters, required):
    for relative in required:
        if not (output / relative).is_file():
            raise FileNotFoundError(output / relative)
    resources = []
    # Runtime-generated CoreML and clone caches are not model resources.
    for file in sorted(output.rglob("*")):
        relative = file.relative_to(output)
        if (not file.is_file() or relative.parts[0] in ("coreml", "prompts")
                or file.name == "manifest.json" or file.name.endswith((".download.lock", ".download", ".corrupt"))):
            continue
        with file.open("rb") as handle:
            digest = hashlib.file_digest(handle, "sha256").hexdigest()
        resources.append({"path": str(relative), "size": file.stat().st_size, "sha256": digest})
    script = pathlib.Path(__file__).with_name(parameters.pop("script"))
    parameters["export_script_sha256"] = hashlib.sha256(script.read_bytes()).hexdigest()
    parameters["manifest_script_sha256"] = hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
    graph_opsets = {}
    for resource in resources:
        if resource["path"].endswith(".onnx"):
            graph = onnx.load(str(output / resource["path"]), load_external_data=False)
            graph_opsets[resource["path"]] = {op.domain or "ai.onnx": op.version for op in graph.opset_import}
    payload = dict(abi_version=1, export_revision=revision, source_revision=source_revision,
                   precision="fp32", parameters=parameters, graph_opsets=graph_opsets, resources=resources)
    (output / "manifest.json").write_text(json.dumps(payload, indent=2) + "\n")
