#!/usr/bin/env python3
"""Verify literal public Lab examples through actual controls, never mark counts alone."""
import argparse
import hashlib
import http.server
import json
import pathlib
import threading


def cases():
    long = 'Word' + ' word' * 28 + ' end.'
    short = 'Word' + ' word' * 27 + ' end.'
    convoluted = ('The plan, which we drafted in May (after the review, which ran late), '
                  'failed; although nobody said so, it was clear.')
    weak = 'Perhaps it is basically fine, really.'
    return [
        ('typos_and_punctuation', 'I recieve teh letter. Tehran is correct.', ['recieve', 'teh']),
        ('typos_and_punctuation', 'I receive the letter. Tehran is correct.', []),
        ('long_sentences', long + ' ' + short, [long]),
        ('long_sentences', short, []),
        ('convoluted_sentences', 'We bought apples, pears, plums, figs, dates and limes. ' + convoluted, [convoluted]),
        ('convoluted_sentences', 'We bought apples, pears, plums, figs, dates and limes.', []),
        ('off_tone', 'The results are awesome and we have lots of stuff to ship.', ['awesome', 'lots of', 'stuff']),
        ('off_tone', 'The results are ready and we have work to ship.', []),
        ('hedges_and_filler', 'This is basically fine and perhaps this seems to work, sort of.', ['basically', 'perhaps', 'seems to', 'sort of']),
        ('hedges_and_filler', 'The engine writes the file.', []),
        ('weakest_sentences', 'Opening line of the note here. ' + weak + ' The engine ranks each sentence by its concepts.', [weak]),
        ('weakest_sentences', weak + '\n\nMaybe it is sort of quite okay, I think.', []),
    ]


def expected_ranges(body, terms):
    """Literal independently authored examples; offsets use the public UTF-16 contract."""
    result = []
    for term in terms:
        at = body.index(term)
        start = len(body[:at].encode('utf-16-le')) // 2
        result.append({'start': start, 'end': start + len(term.encode('utf-16-le')) // 2, 'text': term})
    return sorted(result, key=lambda item: item['start'])


def main():
    from playwright.sync_api import sync_playwright
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--delivery', required=True, type=pathlib.Path)
    parser.add_argument('--manifest', required=True, type=pathlib.Path)
    parser.add_argument('--browser', choices=['chrome', 'firefox'], required=True)
    parser.add_argument('--report', required=True, type=pathlib.Path)
    args = parser.parse_args()
    if args.report.exists():
        raise RuntimeError('Existing evidence report refused')
    manifest = json.loads(args.manifest.read_text())
    # The same independently admitted file bytes used by the recordings.
    files = manifest['assets']
    for name, entry in files.items():
        path = args.delivery / name
        expected = entry['sha256'] if isinstance(entry, dict) else entry
        if path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise RuntimeError('Admitted asset mismatch: ' + name)
    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *pos, **kw):
            super().__init__(*pos, directory=str(args.delivery), **kw)
        def log_message(self, *unused):
            pass
        def do_GET(self):
            name = self.path.split('?', 1)[0].lstrip('/') or 'index.html'
            if name not in files:
                self.send_error(404)
                return
            super().do_GET()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = 'http://127.0.0.1:' + str(server.server_port) + '/'
    report = {'browser': args.browser, 'scope': 'finite literal Lab examples; no general semantic-quality claim',
              'manifestSha256': hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
              'runnerSha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(), 'checks': []}
    try:
        with sync_playwright() as pw:
            engine = pw.chromium if args.browser == 'chrome' else pw.firefox
            browser = engine.launch(**({'channel': 'chrome'} if args.browser == 'chrome' else {}))
            try:
                context = browser.new_context(viewport={'width': 1140, 'height': 720})
                context.route('**/*', lambda route: route.continue_() if route.request.url.startswith(url) else route.abort())
                page = context.new_page()
                page.goto(url, wait_until='networkidle')
                page.wait_for_function('window.terraphimEditor && window.wasmBindings')
                report['browserVersion'] = browser.version
                for action, body, terms in cases():
                    page.evaluate('(text)=>{const e=window.terraphimEditor;e.openDocument(text,"literal-lab-example.md");e.chrome.setMode("write-on");}', body)
                    if not page.evaluate('window.terraphimEditor.lab.isOpen'):
                        page.locator('[data-control="lab"]').click()
                    page.locator('[data-action="' + action + '"]').click()
                    actual = page.evaluate('window.terraphimEditor.decorations.get("lab-marks").map(m=>({start:m.start,end:m.end,text:window.terraphimEditor.surface.getText().slice(m.start,m.end)}))')
                    actual_body = page.evaluate('window.terraphimEditor.surface.getText()')
                    expected = expected_ranges(body, terms)
                    report['checks'].append({'action': action, 'input': body, 'expected': expected, 'actual': actual,
                                             'bodyUnchanged': actual_body == body,
                                             'status': 'PASS' if actual == expected and actual_body == body else 'FAIL'})
            finally:
                browser.close()
    finally:
        server.shutdown()
        report['status'] = 'PASS' if len(report['checks']) == 12 and all(c['status'] == 'PASS' for c in report['checks']) else 'FAIL'
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    if report['status'] != 'PASS':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
