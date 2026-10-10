"""Chaptered production-UI walkthrough; setup is disclosed, outputs are checked."""
import time
import unicodedata
from browser_demo_common import BODY, CAPTION, wait_ready

REQUIREMENTS = [f'R-{section}.{i}' for section,n in [(2,7),(3,6),(4,6),(5,3),(6,5),(7,4),(8,7),(9,3)] for i in range(1,n+1)]


def word_count(text):
    """Independent Python implementation of the documented word convention."""
    def base(c): return c.isalnum() or c == '_'
    n, inside = 0, False
    for i,c in enumerate(text):
        if base(c):
            if not inside: n += 1
            inside = True
        elif unicodedata.category(c).startswith('M') and inside:
            continue
        elif c in "'’-./" and inside and i+1 < len(text) and base(text[i+1]):
            continue
        else: inside = False
    return n


def cut_text(text, ranges):
    """Splice disjoint UTF-16 ranges independently of the editor trim engine."""
    raw = text.encode('utf-16-le')
    previous, parts = 0, []
    for start,end in sorted(ranges):
        if type(start) is not int or type(end) is not int or start < previous or end <= start or end*2 > len(raw):
            raise ValueError('Invalid or overlapping UTF-16 cuts')
        try:
            parts.append(raw[previous*2:start*2].decode('utf-16-le'))
            raw[start*2:end*2].decode('utf-16-le')
        except UnicodeDecodeError as error: raise ValueError('Cut splits surrogate') from error
        previous = end
    try: parts.append(raw[previous*2:].decode('utf-16-le'))
    except UnicodeDecodeError as error: raise ValueError('Cut splits surrogate') from error
    return ''.join(parts)


def preview_text(text, ranges):
    """Status previews count the union, including nested or crossing cuts."""
    merged=[]
    for start,end in sorted(set(tuple(r) for r in ranges)):
        if start < 0 or end <= start: raise ValueError('Invalid preview range')
        if merged and start <= merged[-1][1]: merged[-1]=(merged[-1][0],max(end,merged[-1][1]))
        else: merged.append((start,end))
    return cut_text(text,merged)


def promotion_ranges(ranges):
    """Independent containment-only Done rule; adjacent ranges stay distinct."""
    unique=sorted(set(tuple(r) for r in ranges),key=lambda r:(r[0],-r[1]))
    for i,(start,end) in enumerate(unique):
        for other_start,other_end in unique[i+1:]:
            if start < other_start < end < other_end:
                raise ValueError('Crossing promotion ranges')
    result=[]
    for start,end in unique:
        if start < 0 or end <= start: raise ValueError('Invalid promotion range')
        if result and start < result[-1][1]:
            if end > result[-1][1]: raise ValueError('Crossing promotion ranges')
            continue
        result.append((start,end))
    return result


ANNOTATIONS = 'return window.terraphimEditor.annotations();'
MODE = 'return window.terraphimEditor.chrome.mode;'


def full_storyboard(d, fixture, duration, pause, metadata):
    wait_ready(d)
    started = time.monotonic()
    evidence = metadata.setdefault('outputChecks', [])
    metadata['coverage'] = {rid: {'status': 'NOT DEMONSTRATED', 'checks': []} for rid in REQUIREMENTS}
    metadata['timelineOrigin'] = 'seconds since full storyboard began; capture lead-in recorded separately'
    metadata['setupDisclosure'] = 'Public API opens plain text fixtures and local role before each chapter; subsequent actions use real controls. API reads inspect outputs. Save/reopen bridge explicitly labeled.'
    metadata['storyboard'] = []
    def caption(text):
        d.evaluate(CAPTION, text)
        metadata['storyboard'].append({'seconds':round(time.monotonic()-started,3), 'caption':text})
        pause(1)
    def check(name, actual, expected, ids):
        ok = actual == expected
        index = len(evidence)
        evidence.append({'seconds':round(time.monotonic()-started,3), 'name':name, 'actual':actual, 'expected':expected, 'status':'PASS' if ok else 'FAIL', 'requirements':ids})
        for rid in ids:
            item=metadata['coverage'][rid]; item['checks'].append(index)
            item['status']='OBSERVED CHECKS PASS; FULL CRITERIA REVIEW REQUIRED' if ok else 'FAIL'
        if not ok: raise AssertionError(f'{name}: actual {actual!r} != expected {expected!r}')
    def body(expected, name, ids): check(name, d.evaluate(BODY), expected, ids)
    def setup(text, role=None):
        caption('SETUP — open public fixture; preloaded state is not an operation result.')
        d.evaluate("const e=window.terraphimEditor;e.openDocument(arguments[0],'public-demo.md');e.loadThesaurus(JSON.stringify(arguments[1]));e.chrome.setMode('plain');return true;",text,role or fixture['role'])
        deadline=time.monotonic()+30
        while not d.evaluate('return window.terraphimEditor.graphClient.state === "ready";'):
            if time.monotonic()>deadline: raise RuntimeError('Production graph not ready')
            pause(.25)
        d.click('.te-chrome-counter')
    def tail(text):
        d.click('.markdown-input'); d.keys(['\ue009','\ue010'])
        for _ in text: d.keys(['\ue008','\ue012'])
    def all_body():
        d.click('.markdown-input'); d.keys(['\ue009','\ue011']); d.keys(['\ue009','\ue008','\ue010'])
    def undo(expected, ids):
        d.keys(['\ue009','z']); body(expected,'Undo restores literal body',ids); pause(1)
    def lab_open():
        if not d.evaluate('return window.terraphimEditor.lab.isOpen;'): d.click('[data-control="lab"]')
    def pick(level):
        lab_open(); d.click('[data-level="'+level+'"]'); pause(2)

    setup('# A public headline\n\nPass me a paperclip')
    caption('B01 — Plain mode and Write_On toggle; visible controls, headline and layout.')
    d.click('.te-chrome-counter')
    check('Plain mode via counter',d.evaluate(MODE),'plain',['R-2.1'])
    d.click('.te-chrome-counter')
    check('Write_On via counter',d.evaluate(MODE),'write-on',['R-2.1','R-7.2'])
    visual=d.evaluate("const e=document.querySelector('.markdown-input'),c=getComputedStyle(e),r=e.getBoundingClientRect();return {width:r.width,font:c.fontFamily,color:c.color,lineHeight:c.lineHeight,background:getComputedStyle(document.body).backgroundColor,headline:!!document.querySelector('.te-heading')};")
    metadata['visualMeasurements']=visual
    check('Headline rendered',visual['headline'],True,['R-3.5'])
    pause(3)

    base='Pass me a paperclip'
    setup(base)
    caption('B02/B03 — Create a real word alternative; body updates from the first keystroke.')
    tail('paperclip'); d.keys(['\ue009','\ue008','a'])
    d.click('.te-alt-input--new'); d.text('e')
    body('Pass me an e','First keystroke updates body and article',['R-2.2','R-4.3','R-2.6'])
    pause(2); d.text('raser'); body('Pass me an eraser','Completed candidate before Enter',['R-4.3','R-2.6'])
    d.keys(['\ue007']); pause(2)
    d.click('.te-alt-input--new'); d.text('thumbtack'); d.keys(['\ue007'])
    body('Pass me a thumbtack','Article reverses to a',['R-2.6'])
    check('Original-first ordered created alternatives',d.evaluate('return window.terraphimEditor.altPanel.lines().map(x=>x.text);'),['paperclip','eraser','thumbtack'],['R-2.2','R-4.3'])
    d.click('.te-alt-line[data-index="0"] .te-alt-input'); d.keys(['\ue015'])
    body('Pass me an eraser','Panel Down cycles in context',['R-4.5','R-2.6'])
    d.keys(['\ue013']); body(base,'Panel Up restores original',['R-4.5','R-2.6'])
    d.click('.te-alt-input--new'); d.text('cancelled'); d.keys(['\ue00c'])
    body(base,'Escape cancels new candidate',['R-4.3'])
    pause(2)
    # Public read-only indicator inspection; the alternatives were created above.
    info=d.evaluate("const e=window.terraphimEditor,s=e.annotations().spans[0];return {alts:s.alts.length,active:s.active,decorations:e.decorations.get('indicators').length};")
    metadata['createdWordIndicators']=info
    for kind,text,replacement in [('sentence','A clear original sentence.','A different sentence.'),('paragraph','First sentence. Second sentence.','A different paragraph. Two sentences.')]:
        setup(text); caption('B02 — Create '+kind+' alternatives through the panel, then cycle.')
        all_body(); d.keys(['\ue009','\ue008','a']); d.click('[data-kind="'+kind+'"]')
        d.click('.te-alt-input--new'); d.text(replacement); d.keys(['\ue007'])
        body(replacement,'Created '+kind+' replacement',['R-2.3','R-4.2'])
        check('Created span granularity',d.evaluate(ANNOTATIONS)['spans'][0]['kind'],kind,['R-2.3'])
        pause(3)

    setup(fixture['body'])
    caption('B04 — Local KG generates alternatives; robot provenance is generated output, not fixture setup.')
    tail(fixture['term']); d.keys(['\ue009','\ue008','g']); pause(2)
    check('Generated local alternatives',d.evaluate('return window.terraphimEditor.altPanel.lines().map(x=>x.text);'),[fixture['term'],fixture['replacement']],['R-8.6','R-8.7','R-4.4'])
    check('Generated AI provenance',d.evaluate(ANNOTATIONS)['spans'][0]['alts'][1]['source'],'ai',['R-4.4','R-8.6'])
    d.keys(['\ue00c']); undo(fixture['body'],['R-4.6'])
    check('Undo removes generated span',len(d.evaluate(ANNOTATIONS)['spans']),0,['R-4.6','R-2.7'])

    ghost_body='Keep this. Ghost these words.'
    setup(ghost_body); caption('B05 — Ghost selected text; text and counts remain; Revive restores it.')
    tail('Ghost these words.'); d.keys(['\ue009','/']); pause(3)
    body(ghost_body,'Ghost does not delete text',['R-5.1'])
    check('New ghost created',len(d.evaluate(ANNOTATIONS)['ghosts']),1,['R-5.1'])
    counts=d.evaluate('return window.terraphimEditor.counts();')
    check('Ghost words remain counted',counts['words'],word_count(ghost_body),['R-5.1'])
    metadata['ghostComputedStyle']=d.evaluate("const e=document.querySelector('.te-ghost');return e?{color:getComputedStyle(e).color,opacity:getComputedStyle(e).opacity}:null;")
    d.keys(['\ue009','/']); check('Revive removes ghost',len(d.evaluate(ANNOTATIONS)['ghosts']),0,['R-5.2']); pause(2)

    setup('Alpha beta gamma.'); caption('B06 — Stash moves selected text to Overflow; undo restores both locations.')
    tail('gamma.'); d.keys(['\ue009','\ue008','x']); pause(2)
    body('Alpha beta ','Stash source removal',['R-6.3'])
    check('Stash destination',d.evaluate(ANNOTATIONS).get('overflow',''),'gamma.',['R-6.3','R-6.1'])
    d.click('.markdown-input'); undo('Alpha beta gamma.',['R-6.3'])
    check('Stash undo clears appended Overflow',d.evaluate(ANNOTATIONS).get('overflow',''),'', ['R-6.3'])
    if not d.evaluate('return window.terraphimEditor.overflow.isOpen();'): d.keys(['\ue009','\ue008','d'])
    d.click('.te-overflow-text'); d.text('Public note')
    d.click('.markdown-input'); d.keys(['\ue009','\ue010']); d.click('.te-overflow-text'); d.keys(['\ue009','\ue007']); pause(2)
    body('Alpha beta gamma.\n\nPublic note','Ctrl+Enter pulls paragraph into body',['R-6.2','R-6.4'])
    check('Pull-back preserves Overflow copy',d.evaluate(ANNOTATIONS).get('overflow',''),'Public note',['R-6.4'])

    lab_body=fixture['labBody']
    setup(lab_body,{'name':'public-trim','data':{'unmentioned':{'id':1,'nterm':'unmentioned'}}})
    caption('B08 — Execute all six Lab mark actions; inspect actual marks and unchanged body.')
    for action in ['typos_and_punctuation','weakest_sentences','long_sentences','convoluted_sentences','off_tone','hedges_and_filler']:
        lab_open(); d.click('[data-action="'+action+'"]'); pause(3)
        body(lab_body,action+' never rewrites body',['R-8.1','R-8.2'])
        marks=d.evaluate("return window.terraphimEditor.decorations.get('lab-marks').map(m=>({start:m.start,end:m.end,text:m.data.text,kind:m.data.kind}));")
        check(action+' produces marks',len(marks)>0,True,['R-8.2'])
        evidence.append({'seconds':round(time.monotonic()-started,3),'name':action+' observed marks for semantic review','actual':marks,'status':'OBSERVED; NOT AN INDEPENDENT SEMANTIC ORACLE','requirements':['R-8.2']})
    d.click('.te-lab-clear')
    caption('B09 — Execute every trim level. Python independently splices proposed ranges and counts words.')
    for level in ['original','slight','tighten','sharper','half']:
        pick(level); body(lab_body,'Trim preview preserves exact body: '+level,['R-8.3','R-8.4'])
        if level=='original': continue
        ranges=d.evaluate("return window.terraphimEditor.decorations.get('lab-trim').map(m=>[m.start,m.end]);")
        card=d.evaluate("return document.querySelector('.te-trim-card-count').textContent;")
        before,after=word_count(lab_body),word_count(preview_text(lab_body,ranges))
        check(level+' card counts', [int(x) for x in __import__('re').findall(r'\d+',card)][:2],[before,after],['R-8.4'])
        metadata.setdefault('trimPreviews',[]).append({'level':level,'ranges':ranges,'independentBefore':before,'independentAfter':after,'card':card})
    pick('slight'); d.click('.te-trim-walk-start'); pause(2)
    check('Walk through selects a proposed cut',d.evaluate('const s=window.terraphimEditor.surface.getSelectionOffsets();return s.end>s.start;'),True,['R-8.5'])
    before_keep=d.evaluate('return Array.from(window.terraphimEditor.lab.trim.kept);')
    selected_cut=d.evaluate('const t=window.terraphimEditor.lab.trim;return t.active[t.walkIndex].id;')
    d.click('.te-trim-keep'); pause(2)
    check('Keep records the selected cut',d.evaluate('return Array.from(window.terraphimEditor.lab.trim.kept);'),before_keep+[selected_cut],['R-8.5'])
    check('Keep retains original body',d.evaluate(BODY),lab_body,['R-8.5'])
    remaining_ranges=d.evaluate("return window.terraphimEditor.decorations.get('lab-trim').map(m=>[m.start,m.end]);")
    expected_cut_words=word_count(preview_text(lab_body,remaining_ranges))
    if d.evaluate('return window.terraphimEditor.lab.trim.walkIndex >= 0;'): d.click('.te-trim-stop')
    d.click('.te-trim-cut'); pause(2)
    changed=d.evaluate(BODY)
    check('Make cuts changes body',changed!=lab_body,True,['R-8.5'])
    check('Make cuts produces independently counted remainder',word_count(changed),expected_cut_words,['R-8.5'])
    metadata['makeCutsObservedBody']=changed
    metadata['makeCutsOracleLimit']='Independent count plus changed body; punctuation tidy and every character need additional literal review.'
    d.click('.markdown-input'); undo(lab_body,['R-8.5'])
    pick('slight')
    proposed_ranges=d.evaluate("return window.terraphimEditor.decorations.get('lab-trim').map(m=>[m.start,m.end]);")
    outermost=[list(r) for r in promotion_ranges(proposed_ranges)]
    d.click('.te-trim-done'); pause(3)
    body(lab_body,'Done keeps literal text',['R-8.5'])
    check('Done persists exact outermost preview ranges',sorted([[g['anchor']['start'],g['anchor']['end']] for g in d.evaluate(ANNOTATIONS)['ghosts']]),outermost,['R-8.5','R-5.3'])

    caption('B10 — Serialize newly created ghosts and reopen saved content. API persistence bridge; not OS picker proof.')
    saved=d.evaluate('return window.terraphimEditor.saveDocument();')
    expected_ann=d.evaluate(ANNOTATIONS)
    check('Saved trailing annotation block','```terraphim-alternatives' in saved,True,['R-9.1'])
    d.evaluate("return window.terraphimEditor.openDocument(arguments[0],'public-reopened.md');",saved)
    body(lab_body,'Reopened saved body',['R-9.1','R-5.3'])
    check('Reopened newly created annotations',d.evaluate(ANNOTATIONS),expected_ann,['R-9.1','R-5.3'])
    caption('B10 — Inspect clean Markdown export through the visible export control.')
    d.click('[data-control="markdown"]'); pause(3)
    exported=d.evaluate('return window.terraphimEditor.exportDocument();')
    expected_export=cut_text(lab_body,[(g['anchor']['start'],g['anchor']['end']) for g in expected_ann['ghosts']])
    check('Visible export dialog matches independent output',d.evaluate("return document.querySelector('.te-files-markdown-text').value;"),expected_export,['R-9.3'])
    check('Export drops ghost ranges exactly',exported,expected_export,['R-9.3'])
    check('Export excludes annotation block','terraphim-alternatives' in exported,False,['R-9.3'])
    caption('Coverage: completed actions are checked outputs. Advanced drag, OS picker, ambiguity and pixel review remain separately identified, never implied PASS.')
    metadata['remainingCriteria']=['Trusted drag/drop','OS file-picker save/open','Context reanchor and ambiguity GUI sequence','Full hover/dot geometry and §10 visual review','Shortcut collision GUI sequence','Provider abstraction beyond built-in local provider','Independent semantic assessment of all Lab marks','Full elastic/overlapping ghost edit sequence','Persist and reopen alternative active choice plus Overflow together']
    remaining=duration-(time.monotonic()-started)
    if remaining>0: pause(remaining)
