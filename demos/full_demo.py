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


def overflow_insertion(body, at, text):
    """Independent UTF-16 insertion oracle from the paragraph-spacing contract."""
    raw=body.encode('utf-16-le')
    before=raw[:at*2].decode('utf-16-le')
    after=raw[at*2:].decode('utf-16-le')
    if before.endswith('\r') and after.startswith('\n'):
        before+='\n';after=after[1:]
    if before and not before.endswith('\n') and after and not after.startswith(('\r','\n')):
        return before+text+after
    eol='\r\n' if '\r\n' in body else '\n'
    lead=0 if not before else max(0,2-before[len(before.rstrip('\r\n')):].count('\n')) if before.endswith('\n') else 2
    trail=0 if not after else max(0,2-after[:len(after)-len(after.lstrip('\r\n'))].count('\n')) if after.startswith(('\r','\n')) else 2
    return before+eol*lead+text+eol*trail+after


ANNOTATIONS = 'return window.terraphimEditor.annotations();'
MODE = 'return window.terraphimEditor.chrome.mode;'


def coverage_status(previous, passed):
    """A failed requirement stays failed even when later finite checks pass."""
    return 'FAIL' if previous == 'FAIL' or not passed else 'OBSERVED CHECKS PASS; FULL CRITERIA REVIEW REQUIRED'


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
            item['status']=coverage_status(item['status'], ok)
        if not ok: raise AssertionError(f'{name}: actual {actual!r} != expected {expected!r}')
    def body(expected, name, ids): check(name, d.evaluate(BODY), expected, ids)
    setup_index=0
    def setup(text, role=None):
        nonlocal setup_index
        setup_index+=1
        caption('SETUP — open public fixture; preloaded state is not an operation result.')
        d.evaluate("const e=window.terraphimEditor;e.openDocument(arguments[0],arguments[2]);e.loadThesaurus(JSON.stringify(arguments[1]));e.chrome.setMode('plain');e.rolegraph;return true;",text,role or fixture['role'],f'public-demo-{setup_index}.md')
        deadline=time.monotonic()+30
        while not d.evaluate('return window.terraphimEditor.graphClient?.state === "ready";'):
            if time.monotonic()>deadline: raise RuntimeError('Production graph not ready')
            pause(.25)
        d.click('.te-chrome-counter')
    def tail(text):
        d.click('.markdown-input'); d.keys(['\ue009','\ue010'])
        for _ in text: d.keys(['\ue008','\ue012'])
        check('Selection precondition: exact tail',d.evaluate('const e=window.terraphimEditor,s=e.surface.getSelectionOffsets();return e.surface.getText().slice(s.start,s.end);'),text,[])
    def all_body():
        d.click('.markdown-input'); d.keys(['\ue009','\ue011']); d.keys(['\ue009','\ue008','\ue010'])
        check('Selection precondition: entire body',d.evaluate('const e=window.terraphimEditor,s=e.surface.getSelectionOffsets();return [s.start,s.end];'),[0,len(d.evaluate(BODY).encode('utf-16-le'))//2],[])
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
    group=d.evaluate(ANNOTATIONS)['synonymGroups'][0]
    check('Newly authored alternatives use actual synonyms:: KG syntax',group['markdown'],'synonyms:: paperclip, eraser, thumbtack\n',[])
    members=['eraser','paperclip','thumbtack']
    for term in members:
        check('Actual scoped KG query for '+term,d.evaluate('return JSON.parse(window.wasmBindings.document_synonyms(arguments[0],arguments[1])).sort();',group['spanId'],term),members,[])
    check('Authored aliases share the actual typed KG concept',sorted({v['id'] for v in group['thesaurus']['data'].values()}),[1],[])
    metadata['authoredSynonymRegistration']={'group':group,'source':'real UI-created alternatives; actual shared WASM query outputs'}
    caption('Verified KG output — '+group['markdown'].strip()+'; all three terms resolve to the same scoped concept.')
    d.click('.te-alt-line[data-index="0"] .te-alt-input'); d.keys(['\ue015'])
    body('Pass me an eraser','Panel Down cycles in context',['R-4.5','R-2.6'])
    d.keys(['\ue013']); body(base,'Panel Up restores original',['R-4.5','R-2.6'])
    d.click('.te-alt-input--new'); d.text('cancelled'); d.keys(['\ue00c'])
    body(base,'Escape cancels new candidate',['R-4.3'])
    pause(2)
    # Public read-only indicator inspection; the alternatives were created above.
    info=d.evaluate("const e=window.terraphimEditor,s=e.annotations().spans[0];return {alts:s.alts.length,active:s.active,decorations:e.decorations.get('indicators').length};")
    metadata['createdWordIndicators']=info
    d.keys(['\ue00c']); d.click('.markdown-input'); d.hover('.te-ind-word'); d.keys(['\ue015']); pause(2)
    body('Pass me an eraser','Hover over word plus Down cycles actual body',['R-2.4','R-3.6'])
    dots=d.evaluate("const h=document.querySelector('.te-ind-row[data-kind=word]'),t=document.querySelector('.te-ind-word'),c=getComputedStyle(t);return {count:h.querySelectorAll('.te-ind-dot').length,active:Array.from(h.querySelectorAll('.te-ind-dot--active')).map(x=>Number(x.dataset.index)),colors:Array.from(h.querySelectorAll('.te-ind-dot')).map(x=>getComputedStyle(x).backgroundColor),dotVisible:Array.from(h.querySelectorAll('.te-ind-dot')).every(x=>x.getBoundingClientRect().width>0&&x.getBoundingClientRect().height>0&&getComputedStyle(x).visibility==='visible'),alignment:Math.abs((h.getBoundingClientRect().left+h.getBoundingClientRect().width/2)-(t.getBoundingClientRect().left+t.getBoundingClientRect().width/2)),underline:c.textDecorationLine,thickness:c.textDecorationThickness,visible:!h.hidden};")
    check('Word has one visible dot per alternative',dots['count'],3,['R-3.2'])
    check('Exactly the chosen dot is bright',dots['active'],[1],['R-3.3'])
    check('Word underline rendered at one pixel',[dots['underline'],dots['thickness']],['underline','1px'],['R-3.1'])
    check('Dot row has rendered visible dots',[dots['visible'],dots['dotVisible']],[True,True],['R-3.2'])
    check('Word dots are centred within two pixels',dots['alignment']<=2,True,['R-3.2'])
    check('Active dot uses full lavender and dim dots differ',[dots['colors'][1],dots['colors'][0]!=dots['colors'][1],dots['colors'][2]!=dots['colors'][1]],['rgb(140, 134, 230)',True,True],['R-3.3'])
    metadata['wordIndicatorMeasurements']=dots
    d.keys(['\ue013']); body(base,'Hover Up restores original',['R-2.4'])
    for kind,text,replacement in [('sentence','A clear original sentence.','A different sentence.'),('paragraph','First sentence. Second sentence.','A different paragraph. Two sentences.')]:
        setup(text); caption('B02 — Create '+kind+' alternatives through the panel, then cycle.')
        all_body(); d.keys(['\ue009','\ue008','a']); d.click('[data-kind="'+kind+'"]')
        d.click('.te-alt-input--new'); d.text(replacement); d.keys(['\ue007'])
        body(replacement,'Created '+kind+' replacement',['R-2.3','R-4.2'])
        check('Created span granularity',d.evaluate(ANNOTATIONS)['spans'][0]['kind'],kind,['R-2.3'])
        marker=d.evaluate("const kind=arguments[0],h=document.querySelector('[data-span-id][data-kind='+kind+']'),t=document.querySelector('.te-ind-'+kind),r=h.getBoundingClientRect(),tr=t.getBoundingClientRect();return {dots:h.querySelectorAll('.te-ind-dot').length,active:h.querySelectorAll('.te-ind-dot--active').length,visible:!h.hidden,rule:!!h.querySelector('.te-ind-rule'),underline:getComputedStyle(t).textDecorationLine,height:r.height,textHeight:tr.height,rightDifference:Math.abs(r.right-tr.right),dotVisible:Array.from(h.querySelectorAll('.te-ind-dot')).every(x=>x.getBoundingClientRect().width>0&&x.getBoundingClientRect().height>0&&getComputedStyle(x).visibility==='visible'),gutter:(()=>{const q=h.querySelector('.te-ind-rule'),dots=h.querySelector('.te-ind-col');if(!q)return null;const qr=q.getBoundingClientRect(),dr=dots.getBoundingClientRect();return {width:qr.width,height:qr.height,leftOfText:tr.left-qr.right,dotsLeft:dr.right<=qr.left};})()};",kind)
        check(kind+' indicators visible with exactly two choices and one active',[marker['visible'],marker['dots'],marker['active']],[True,2,1],['R-3.4' if kind=='paragraph' else 'R-3.2','R-3.3'])
        check(kind+' dots have visible rendered area',marker['dotVisible'],True,['R-3.4' if kind=='paragraph' else 'R-3.2'])
        if kind=='paragraph':
            check('Paragraph gutter rule replaces underline',[marker['rule'],marker['underline']],[True,'none'],['R-3.4'])
            g=marker['gutter'];check('Paragraph one-pixel gutter spans text height with dots on its left',[g['width'],abs(g['height']-marker['textHeight'])<=2,10<=g['leftOfText']<=30,g['dotsLeft']],[1,True,True,True],['R-3.4'])
        else: check('Sentence dots end at underline within two pixels',marker['rightDifference']<=2,True,['R-3.2'])
        metadata.setdefault('granularityIndicatorMeasurements',{})[kind]=marker
        pause(3)

    setup(fixture['body'])
    caption('B04 — Local KG generates alternatives; robot provenance is generated output, not fixture setup.')
    tail(fixture['term']); d.keys(['\ue008','\ue03a']); d.click('.te-selection-menu-item[data-item="ai-alternatives"]'); pause(2)
    check('Generated local alternatives',d.evaluate('return window.terraphimEditor.altPanel.lines().map(x=>x.text);'),[fixture['term'],fixture['replacement']],['R-8.6','R-8.7','R-4.4'])
    check('Generated AI provenance',d.evaluate(ANNOTATIONS)['spans'][0]['alts'][1]['source'],'ai',['R-4.4','R-8.6'])
    generated_span_id=d.evaluate(ANNOTATIONS)['spans'][0]['id']
    d.keys(['\ue00c']); undo(fixture['body'],['R-4.6'])
    check('Undo removes generated span',len(d.evaluate(ANNOTATIONS)['spans']),0,['R-4.6','R-2.7'])
    check('Undo removes generated KG synonym membership',d.evaluate(ANNOTATIONS)['synonymGroups'],[],[])
    check('Undo removes the authored inline indicator row',d.evaluate("return Array.from(document.querySelectorAll('.te-ind-row')).filter(n=>n.dataset.spanId===arguments[0]).length;",generated_span_id),0,['R-4.6','R-2.7'])
    check('Undo query removes the authored synonym group',d.evaluate('return JSON.parse(window.wasmBindings.document_synonyms(arguments[0],arguments[1]));',generated_span_id,fixture['term']),[],[])
    metadata['generatedUndoRemainingIndicators']=d.evaluate("return Array.from(document.querySelectorAll('.te-ind-row')).map(n=>({spanId:n.dataset.spanId,source:n.dataset.source}));")

    ghost_body='Keep this. Ghost these words.'
    setup(ghost_body); caption('B05 — Ghost selected text; text and counts remain; Revive restores it.')
    tail('Ghost these words.'); d.keys(['\ue008','\ue03a']); d.click('.te-selection-menu-item[data-item="ghost"]'); pause(3)
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

    drag_body='First paragraph.\n\nSecond paragraph.\n\nThird paragraph.'
    setup(drag_body)
    caption('B06 — Native selected-text drag from Overflow into real rendered text; independently check insertion and undo.')
    d.click('.markdown-input')
    check('Pull-drag shortcut targets the focused editor',d.evaluate('return window.terraphimEditor.surface.root.contains(document.activeElement);'),True,[])
    check('Pull-drag fixture starts with Overflow closed',d.evaluate('return window.terraphimEditor.overflow.isOpen();'),False,[])
    d.keys(['\ue009','\ue008','d']);d.click('.te-overflow-text');d.text('Public note.')
    d.keys(['\ue03d' if d.evaluate('return navigator.platform.includes("Mac");') else '\ue009','a'])
    check('Native Overflow selection precondition',d.evaluate("const a=document.querySelector('.te-overflow-text');return a.value.slice(a.selectionStart,a.selectionEnd);"),'Public note.',[])
    drag_observer="""window.__demoDrag=[];window.__demoDragAbort?.abort();window.__demoDragAbort=new AbortController();for(const type of ['dragstart','drop'])document.addEventListener(type,e=>{const t=e.target.nodeType===3?e.target.parentElement:e.target;window.__demoDrag.push({type,trusted:e.isTrusted,target:t.closest('.te-overflow')?'overflow':t.closest('[contenteditable]')?'surface':'other',payload:type==='drop'?e.dataTransfer.getData('text/plain'):null,offset:type==='drop'&&t.closest('[contenteditable]')?window.terraphimEditor.surface.dropOffset(e):null});},{capture:true,signal:window.__demoDragAbort.signal});return true;"""
    d.evaluate(drag_observer)
    geometry=d.evaluate("const a=document.querySelector('.te-overflow-text').getBoundingClientRect(),root=window.terraphimEditor.surface.root,n=document.createTreeWalker(root,NodeFilter.SHOW_TEXT).nextNode(),r=document.createRange();r.setStart(n,0);r.setEnd(n,1);const b=r.getBoundingClientRect();return {start:[a.x+12,a.y+12],end:[b.x+b.width/2,b.y+b.height/2]};")
    d.drag(geometry['start'],geometry['end']);pause(2)
    receipts=d.evaluate('return window.__demoDrag;');metadata['nativePullDragReceipts']=receipts
    try:
        check('Native pull drag has one trusted source and destination',[[r['type'],r['trusted'],r['target']] for r in receipts],[['dragstart',True,'overflow'],['drop',True,'surface']],['R-6.4'])
    except AssertionError as error:
        if not metadata.get('recordFailedDragChapters'):
            raise
        metadata['acceptanceStatus'] = 'FAIL'
        metadata.setdefault('failedDragChapters', []).append({'chapter': 'pull drag',
            'failure': str(error), 'dependentChecks': [{'name': name, 'status': 'NOT EXECUTED'} for name in ['Exact drop payload', 'Exact inserted body', 'Overflow copy', 'Undo', 'Redo']]})
        caption('FAILED — pull drag: actual output check failed; dependent checks NOT EXECUTED. Continuing independent chapters.')
        pause(2)
        d.keys(['\ue00c'])
        d.evaluate('window.__demoDragAbort?.abort();return true;')
    else:
        drop=receipts[-1];check('Native pull drop carries exact selected text',drop['payload'],'Public note.',['R-6.4'])
        drag_expected=overflow_insertion(drag_body,drop['offset'],'Public note.')
        body(drag_expected,'Native pull inserts independently separated text',['R-6.4'])
        check('Native pull keeps Overflow copy',d.evaluate(ANNOTATIONS)['overflow'],'Public note.',['R-6.4'])
        d.click('.markdown-input');undo(drag_body,['R-6.4']);d.keys(['\ue009','\ue008','z']);body(drag_expected,'Native pull redo restores exact body',['R-6.4'])

    setup(drag_body);caption('B06 — Native drag of the last paragraph into Overflow; verify move, append, undo and redo.')
    d.click('.markdown-input')
    check('Stash-drag shortcut targets the focused editor',d.evaluate('return window.terraphimEditor.surface.root.contains(document.activeElement);'),True,[])
    check('Stash-drag fixture starts with Overflow closed',d.evaluate('return window.terraphimEditor.overflow.isOpen();'),False,[])
    d.keys(['\ue009','\ue008','d']);d.click('.te-overflow-text');d.text('Existing note.');tail('Third paragraph.')
    d.evaluate(drag_observer)
    geometry=d.evaluate("const r=document.getSelection().getRangeAt(0).getClientRects()[0],b=document.querySelector('.te-overflow-text').getBoundingClientRect();return {start:[r.x+r.width/2,r.y+r.height/2],end:[b.x+20,b.y+20]};")
    d.drag(geometry['start'],geometry['end']);pause(2)
    receipts=d.evaluate('return window.__demoDrag;');metadata['nativeStashDragReceipts']=receipts
    try:
        check('Native stash drag has trusted source and destination',[[r['type'],r['trusted'],r['target']] for r in receipts],[['dragstart',True,'surface'],['drop',True,'overflow']],['R-6.3','R-6.4'])
    except AssertionError as error:
        if not metadata.get('recordFailedDragChapters'):
            raise
        metadata['acceptanceStatus'] = 'FAIL'
        metadata.setdefault('failedDragChapters', []).append({'chapter': 'stash drag',
            'failure': str(error), 'dependentChecks': [{'name': name, 'status': 'NOT EXECUTED'} for name in ['Exact source removal', 'Exact Overflow append', 'Undo source and Overflow', 'Redo']]})
        caption('FAILED — stash drag: actual output check failed; dependent checks NOT EXECUTED. Continuing independent chapters.')
        pause(2)
        d.keys(['\ue00c'])
        d.evaluate('window.__demoDragAbort?.abort();return true;')
    else:
        body('First paragraph.\n\nSecond paragraph.\n\n','Native stash removes exact source paragraph',['R-6.3'])
        check('Native stash appends exact text after blank line',d.evaluate(ANNOTATIONS)['overflow'],'Existing note.\n\nThird paragraph.',['R-6.3'])
        d.click('.markdown-input');undo(drag_body,['R-6.3']);check('Native stash undo restores Overflow',d.evaluate(ANNOTATIONS)['overflow'],'Existing note.',['R-6.3'])
        d.keys(['\ue009','\ue008','z']);body('First paragraph.\n\nSecond paragraph.\n\n','Native stash redo removes source again',['R-6.3'])


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
    walk=d.evaluate("const t=window.terraphimEditor.lab.trim,c=t.active[t.walkIndex],nodes=Array.from(document.querySelectorAll('.te-trim-current'));return {expected:window.terraphimEditor.surface.getText().slice(c.start,c.end),visible:nodes.map(n=>n.textContent).join(''),count:nodes.length};")
    check('Walk through visibly highlights the exact proposed cut',walk['visible'],walk['expected'],['R-8.5'])
    check('Walk through renders a current-cut marker',walk['count']>0,True,['R-8.5'])
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
    if d.evaluate(MODE)=='plain': d.click('.te-chrome-counter')
    d.click('[data-control="markdown"]'); pause(3)
    exported=d.evaluate('return window.terraphimEditor.exportDocument();')
    expected_export=cut_text(lab_body,[(g['anchor']['start'],g['anchor']['end']) for g in expected_ann['ghosts']])
    check('Visible export dialog matches independent output',d.evaluate("return document.querySelector('.te-files-markdown-text').value;"),expected_export,['R-9.3'])
    check('Export drops ghost ranges exactly',exported,expected_export,['R-9.3'])
    check('Export excludes annotation block','terraphim-alternatives' in exported,False,['R-9.3'])
    d.keys(['\ue00c'])
    combined='Keep this. Pass me a paperclip'
    setup(combined)
    caption('B11 — Create a choice, ghost and Overflow note, then save and reopen all three together.')
    tail('paperclip'); d.keys(['\ue009','\ue008','a'])
    d.click('.te-alt-input--new'); d.text('eraser'); d.keys(['\ue007']); d.keys(['\ue00c'])
    combined_active='Keep this. Pass me an eraser'
    body(combined_active,'Combined fixture choice is actual typed output',['R-2.2','R-2.6'])
    tail('Pass me an eraser'); d.keys(['\ue009','/']); pause(2)
    check('Ghost covers the sentence containing the alternative',[[g['anchor']['start'],g['anchor']['end']] for g in d.evaluate(ANNOTATIONS)['ghosts']],[[11,len(combined_active)]],['R-5.3'])
    tail('eraser'); d.keys(['\ue009','\ue008','a'])
    d.click('.te-alt-line[data-index="1"] .te-alt-input'); d.keys(['\ue013']); pause(2)
    body(combined,'Swap within ghost restores original wording',['R-5.3','R-2.6'])
    check('Ghost expands with longer original',[[g['anchor']['start'],g['anchor']['end']] for g in d.evaluate(ANNOTATIONS)['ghosts']],[[11,len(combined)]],['R-5.3'])
    d.keys(['\ue015']); pause(2)
    body(combined_active,'Swap within ghost restores active choice',['R-5.3'])
    check('Ghost contracts with shorter choice',[[g['anchor']['start'],g['anchor']['end']] for g in d.evaluate(ANNOTATIONS)['ghosts']],[[11,len(combined_active)]],['R-5.3'])
    d.keys(['\ue00c']); d.keys(['\ue009','\ue008','d']); d.click('.te-overflow-text'); d.text('A saved public note.'); d.click('.markdown-input'); pause(2)
    combined_ann=d.evaluate(ANNOTATIONS)
    check('Typed Overflow reaches document model',combined_ann['overflow'],'A saved public note.',['R-6.2','R-6.5'])
    check('Active choice persisted in model',combined_ann['spans'][0]['active'],1,['R-2.5','R-9.2'])
    combined_saved=d.evaluate('return window.terraphimEditor.saveDocument();')
    d.evaluate("return window.terraphimEditor.openDocument(arguments[0],'public-combined-reopened.md');",combined_saved)
    body(combined_active,'Reopen preserves chosen body',['R-2.5','R-9.1'])
    check('Reopen preserves full choice, ghost and Overflow state',d.evaluate(ANNOTATIONS),combined_ann,['R-2.5','R-5.3','R-6.5','R-9.2'])
    reopened_group=d.evaluate(ANNOTATIONS)['synonymGroups'][0]
    for term in ['paperclip','eraser']:
        check('Fresh reopened authored KG query for '+term,d.evaluate('return JSON.parse(window.wasmBindings.document_synonyms(arguments[0],arguments[1])).sort();',reopened_group['spanId'],term),['eraser','paperclip'],[])
    caption('Verified reopened KG output — '+reopened_group['markdown'].strip()+'. Choice, ghost and Overflow were restored together.')
    check('Combined clean export excludes ghost and Overflow',d.evaluate('return window.terraphimEditor.exportDocument();'),'Keep this.',['R-9.3'])
    pause(3)
    caption('B12 — Merge overlapping ghost coverage, revive an interior selection, then delete the ghosted text.')
    if d.evaluate(MODE)=='plain': d.click('.te-chrome-counter')
    all_body(); d.keys(['\ue009','/']); pause(2)
    check('Overlapping coverage merges into one whole-body ghost',[[g['anchor']['start'],g['anchor']['end']] for g in d.evaluate(ANNOTATIONS)['ghosts']],[[0,len(combined_active)]],['R-5.3'])
    d.click('.markdown-input'); d.keys(['\ue009','\ue011'])
    for _ in range(11): d.keys(['\ue014'])
    for _ in range(7): d.keys(['\ue008','\ue014'])
    d.keys(['\ue008','\ue03a']); pause(2)
    check('Selection menu offers Revive',d.evaluate("return document.querySelector('.te-selection-menu-item[data-item=ghost] .te-selection-menu-label')?.textContent;"),'Revive',['R-5.2','R-7.3'])
    d.click('.te-selection-menu-item[data-item="ghost"]'); pause(2)
    check('Reviving interior text splits the ghost',[[g['anchor']['start'],g['anchor']['end']] for g in d.evaluate(ANNOTATIONS)['ghosts']],[[0,11],[18,len(combined_active)]],['R-5.3'])
    all_body(); d.keys(['\ue003']); pause(2)
    body('','Deleting the entire body removes its text',['R-5.3'])
    check('Deleting all ghost text removes ghost ranges',d.evaluate(ANNOTATIONS)['ghosts'],[],['R-5.3'])
    caption('Coverage: completed actions are checked outputs. Advanced drag, OS picker, ambiguity and pixel review remain separately identified, never implied PASS.')
    metadata['remainingCriteria']=['Trusted drag/drop','OS file-picker save/open','Context reanchor and ambiguity GUI sequence','Full hover/dot geometry and §10 visual review','Shortcut collision GUI sequence','Provider abstraction beyond built-in local provider','Independent semantic assessment of all Lab marks','Editing inside ghost beyond alternative swaps']
    remaining=duration-(time.monotonic()-started)
    if remaining>0: pause(remaining)
    metadata['acceptanceStatus'] = 'FAIL' if metadata.get('acceptanceStatus') == 'FAIL' or metadata.get('failedDragChapters') or any(item['status'] == 'FAIL' for item in evidence) else 'FINITE CHECKS PASS; FULL REQUIREMENTS NOT ACCEPTED'
