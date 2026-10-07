"""Rebuild ID fixtures with SentencePiece and the pinned official character wrapper."""
import argparse
import importlib.util
import json
import sys
from pathlib import Path

import sentencepiece


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    tokenizer_source = parser.add_mutually_exclusive_group(required=True)
    tokenizer_source.add_argument("--sentencepiece", type=Path)
    tokenizer_source.add_argument("--gguf", type=Path)
    tokenizer_source.add_argument("--original-tokenizer", type=Path)
    parser.add_argument("--gguf-python", type=Path)
    parser.add_argument("--fixtures", type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("official_voxcpm_utils", args.source / "src/voxcpm/model/utils.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    if args.original_tokenizer:
        from transformers import LlamaTokenizerFast
        tokenizer = module.mask_multichar_chinese_tokens(
            LlamaTokenizerFast.from_pretrained(str(args.original_tokenizer), local_files_only=True))
        fixtures = json.loads(args.fixtures.read_text(encoding="utf-8"))
        changed = 0
        for item in fixtures:
            ids = tokenizer(item["text"])
            changed += ids != item["ids"]
            item["ids"] = ids
        args.fixtures.write_text(json.dumps(fixtures, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"Validated {len(fixtures)} original HF fixtures; {changed} ID lists changed")
        return
    if args.sentencepiece:
        processor = sentencepiece.SentencePieceProcessor(model_file=str(args.sentencepiece))
    else:
        if args.gguf_python:
            sys.path.insert(0, str(args.gguf_python))
        from gguf import GGUFReader
        from sentencepiece import sentencepiece_model_pb2

        fields = GGUFReader(str(args.gguf)).fields
        pieces = fields["tokenizer.ggml.tokens"].contents()
        scores = fields["tokenizer.ggml.scores"].contents()
        types = fields["tokenizer.ggml.token_type"].contents()
        proto = sentencepiece_model_pb2.ModelProto()
        proto.trainer_spec.model_type = sentencepiece_model_pb2.TrainerSpec.BPE
        proto.trainer_spec.vocab_size = len(pieces)
        proto.trainer_spec.byte_fallback = True
        proto.normalizer_spec.name = "identity"
        proto.normalizer_spec.add_dummy_prefix = True
        proto.normalizer_spec.remove_extra_whitespaces = False
        proto.normalizer_spec.escape_whitespaces = True
        for text, score, kind in zip(pieces, scores, types, strict=True):
            proto.pieces.add(piece=text, score=score, type=kind)
        processor = sentencepiece.SentencePieceProcessor(model_proto=proto.SerializeToString())

    class BaseTokenizer:
        vocab = {processor.id_to_piece(i): i for i in range(processor.get_piece_size())}

        def tokenize(self, text, **kwargs):
            return processor.encode(text, out_type=str)

        def convert_tokens_to_ids(self, tokens):
            return [self.vocab[token] for token in tokens]

    tokenizer = module.mask_multichar_chinese_tokens(BaseTokenizer())
    fixtures = json.loads(args.fixtures.read_text(encoding="utf-8"))
    changed = 0
    for item in fixtures:
        ids = tokenizer(item["text"])
        changed += ids != item["ids"]
        item["ids"] = ids
    args.fixtures.write_text(json.dumps(fixtures, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Validated {len(fixtures)} fixtures; {changed} ID lists changed")


if __name__ == "__main__":
    main()
