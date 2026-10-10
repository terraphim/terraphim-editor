#!/usr/bin/env python3
"""Real Chrome/Firefox capture; existing Python Playwright/runtime only."""
import pathlib
import time
from browser_demo_common import *


class Driver:
    def __init__(self, page):
        self.page = page
    def evaluate(self, script, *args):
        return self.page.evaluate('(args) => { return (function(){' + script + '}).apply(null,args); }', list(args))
    def click(self, selector):
        self.page.locator(selector).first.click(timeout=10000)
    def hover(self, selector):
        self.page.locator(selector).first.hover(timeout=10000)
    def drag(self, start, end):
        self.page.mouse.move(*start); self.page.mouse.down()
        self.page.wait_for_timeout(500)
        self.page.mouse.move(start[0]+12,start[1],steps=6)
        self.page.mouse.move(*end,steps=18); self.page.wait_for_timeout(250)
        self.page.mouse.move(end[0]+1,end[1],steps=2); self.page.wait_for_timeout(100)
        self.page.mouse.up(); self.page.wait_for_timeout(100)
    def text(self, text):
        self.page.keyboard.type(text, delay=55)
    def keys(self, keys):
        keys=native_boundary_keys(keys,self.evaluate("return navigator.platform.includes(\"Mac\");"))
        mapping = {'\ue009': 'Control', '\ue03d':'Meta', '\ue008': 'Shift', '\ue010': 'End',
                   '\ue011': 'Home', '\ue007': 'Enter', '\ue013': 'ArrowUp', '\ue012': 'ArrowLeft', '\ue014':'ArrowRight', '\ue015': 'ArrowDown', '\ue00c': 'Escape', '\ue003':'Backspace', '\ue03a':'F10'}
        self.page.keyboard.press('+'.join(mapping.get(k, k) for k in keys))


def main():
    args = parser(True).parse_args()
    from playwright.sync_api import sync_playwright
    with inputs(args, args.browser) as (stage, url, fixture, metadata, ffmpeg, ffprobe):
        with sync_playwright() as pw:
            engine = pw.chromium if args.browser == 'chrome' else pw.firefox
            # Playwright's own executable resolver proves its video encoder exists.
            from playwright._impl._driver import compute_driver_executable
            import os
            env = dict(os.environ)
            env['PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD'] = '1'
            node, cli = compute_driver_executable()
            bundle = pathlib.Path(cli).parent / 'lib/coreBundle.js'
            legacy = pathlib.Path(cli).parent / 'lib/server/registry/index.js'
            registry_module = bundle if bundle.is_file() else legacy
            if not registry_module.is_file():
                raise RuntimeError('Unknown Playwright executable registry; qualification required')
            registry = run([str(node), '-e',
                "const m=require(process.argv[1]);const r=m.registry.registry || m.registry;console.log(r.findExecutable('ffmpeg').executablePath());",
                str(registry_module)], env=env).strip()
            if not pathlib.Path(registry).is_file():
                raise RuntimeError('Cached Playwright video encoder absent; no downloads')
            metadata['playwrightRecordingEncoder'] = encoder_identity(registry)
            if args.browser == 'firefox' and not pathlib.Path(engine.executable_path).is_file():
                raise RuntimeError('Existing Firefox runtime absent; configure PLAYWRIGHT_BROWSERS_PATH')
            browser = engine.launch(**({'channel': 'chrome'} if args.browser == 'chrome' else {}))
            context = None
            errors = []
            try:
                context = browser.new_context(viewport=VIEWPORT, record_video_dir=str(stage / 'video'), record_video_size=VIEWPORT)
                context.route('**/*', lambda route: route.continue_() if route.request.url.startswith(url) else route.abort())
                page = context.new_page()
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.goto(url, wait_until='networkidle')
                metadata['userAgent'] = page.evaluate('navigator.userAgent')
                metadata['browserVersion'] = browser.version
                metadata['captureMethod'] = 'Playwright viewport recording; isolated context'
                if getattr(args, 'full_functionality', False):
                    from full_demo import full_storyboard
                    full_storyboard(Driver(page), fixture, args.duration, lambda sec: page.wait_for_timeout(sec * 1000), metadata)
                else:
                    storyboard(Driver(page), fixture, args.duration, lambda sec: page.wait_for_timeout(sec * 1000))
                if errors:
                    raise RuntimeError('Production page errors during recording')
                video = page.video
                context.close()
                context = None
                if encoder_identity(registry) != metadata['playwrightRecordingEncoder']:
                    raise RuntimeError('Playwright recording encoder drift')
                encode(ffmpeg, ffprobe, ['-i', video.path()], args.output, metadata)
            finally:
                if context:
                    context.close()
                browser.close()


if __name__ == '__main__':
    main()
