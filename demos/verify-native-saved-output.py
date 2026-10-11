"""Verify the public native VS Code ghost regression against actual saved bytes.

This does not drive the editor or create its output. The file must be saved by
the running extension after creating the three alternatives and ghosting the
exact phrase through native input.
"""
import argparse
import hashlib
import json
import pathlib
import re


def verify(path):
    raw = pathlib.Path(path).read_bytes()
    source = raw.decode('utf-8')
    match = re.search(r'\n\n```terraphim-alternatives\n(.*?)\n```\s*$', source, re.S)
    assert match, 'Saved annotation block is missing'
    body, annotations = source[:match.start()], json.loads(match.group(1))
    assert body == 'Pass me a thumbtack.\n\nKeep this sentence. Ghost this aside.\n\nMove this paragraph to Overflow.\n', repr(body)
    assert annotations['version'] == 1
    assert len(annotations['spans']) == 1
    span = annotations['spans'][0]
    assert span['active'] == 2
    assert [(a['text'], a['source']) for a in span['alts']] == [
        ('paperclip', 'original'), ('eraser', 'human'), ('thumbtack', 'human')]
    assert span['anchor']['text'] == body[span['anchor']['start']:span['anchor']['end']] == 'thumbtack'
    assert len(annotations['ghosts']) == 1, 'Ghost changed visibly but was not saved'
    anchor = annotations['ghosts'][0]['anchor']
    assert anchor['text'] == body[anchor['start']:anchor['end']] == 'Ghost this aside.'
    return {'result': 'PASS', 'path': str(pathlib.Path(path).resolve()),
            'sha256': hashlib.sha256(raw).hexdigest(), 'body': body,
            'annotations': annotations,
            'qualification': 'Finite actual saved-output regression; does not qualify all 41 requirements or native KG query'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('path', type=pathlib.Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.path), indent=2))
