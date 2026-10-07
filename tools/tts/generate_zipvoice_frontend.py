"""Development-only asset generation; release inference has no Python dependency.
Requires jieba==0.42.1 pypinyin==0.55.0 cn2an==0.5.24.
The upstream Jieba MIT license is retained separately when wheels omit it.
"""
from pathlib import Path
import hashlib,json,jieba,pypinyin,cn2an,shutil
from pypinyin import lazy_pinyin,Style
from pypinyin.contrib.tone_convert import to_initials,to_finals_tone3
from pypinyin.constants import PINYIN_DICT,PHRASES_DICT

root=Path(__file__).resolve().parents[2]/'crates/novel-tts-backends/src/zipvoice/frontend/assets';root.mkdir(parents=True,exist_ok=True)
dictionary=Path(jieba.__file__).parent/'dict.txt'
shutil.copyfile(dictionary,root/'jieba.dict')
words={line.split()[0] for line in dictionary.read_text(encoding='utf-8').splitlines()}
words.update(PHRASES_DICT)
words.update(chr(code) for code in PINYIN_DICT)
phones={}; pronunciations={}; characters={}
def split(syllable):
    if len(syllable)>1 and syllable[:-1].isalpha() and syllable[-1] in '12345':
        initial=to_initials(syllable,strict=False)
        final=to_finals_tone3(syllable,strict=False,neutral_tone_with_five=True)
        return ([initial+'0'] if initial else [])+([final] if final else [])
    return [syllable]
for index,word in enumerate(sorted(words)):
    if not any('\u3400'<=c<='\u9fff' for c in word):continue
    syllables=lazy_pinyin([word],style=Style.TONE3,tone_sandhi=True,neutral_tone_with_five=True)
    pronunciations[word]=[phone for s in syllables for phone in split(s)]
    for s in syllables:phones[s]=split(s)
    if len(word)==1: characters[word]=lazy_pinyin(word,style=Style.TONE3,tone_sandhi=False,neutral_tone_with_five=True)[0]
    if index%10000==0:print(index,flush=True)
for s in list(phones):
    if len(s)>1 and s[:-1].isalpha() and s[-1] in '12345':
        for tone in '12345':phones[s[:-1]+tone]=split(s[:-1]+tone)
for filename,data in [('words.json',pronunciations),('syllables.json',phones),('characters.json',characters)]:
    (root/filename).write_text(json.dumps(data,ensure_ascii=False,separators=(',',':')),encoding='utf-8')
for name,module in [('jieba',jieba),('pypinyin',pypinyin),('cn2an',cn2an)]:
    package=Path(module.__file__).parent
    dist=next(package.parent.glob(name+'-*.dist-info'))
    candidates=[p for p in list(dist.glob('**/LICENSE*'))+list(dist.glob('**/COPYING*')) if p.is_file()]
    if candidates:shutil.copyfile(candidates[0],root/(name+'-LICENSE'))
manifest={p.name:{'size':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in root.iterdir() if p.is_file() and p.name != "manifest.json"}
(root/'manifest.json').write_text(json.dumps(manifest,indent=2),encoding='utf-8')
print('dictionary assets complete',flush=True)
