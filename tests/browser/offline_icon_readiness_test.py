"""Symbolic event-order unit tests, NOT real browser qualification.

The actual oracle is executed by Node with deterministic clock/DOM doubles.
Real installed-Chrome evidence is recorded separately in the verification note.
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('offline_icons_unit', ROOT / 'scripts/ci/editor-offline-check.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

HARNESS = r'''
const vm = require('node:vm');
const {source, scenario} = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
let clock = 0, id = 0; const tasks = new Map();
const setTimeout = (fn, delay) => { tasks.set(++id, {fn, at:clock + delay}); return id; };
const clearTimeout = id => tasks.delete(id);
class SVGSVGElement {
  namespaceURI = 'http://www.w3.org/2000/svg';
  querySelector() { return {}; }
  getBoundingClientRect() { return {width:16, height:16}; }
}
const listeners = new Map(); let svg = null;
const icon = {name:'type-bold', shadowRoot:{querySelector:() => svg},
  addEventListener:(name, fn) => listeners.set(name, fn),
  removeEventListener:(name, fn) => { if (listeners.get(name) === fn) listeners.delete(name); }};
const emit = name => listeners.get(name)?.();
const ready = vm.runInNewContext(source, {setTimeout, clearTimeout, SVGSVGElement, Error});
let outcome = 'pending';
ready(icon, () => {
  if (scenario === 'cached') { emit('sl-load'); setTimeout(() => svg = new SVGSVGElement(), 1); }
  if (scenario === 'cold') setTimeout(() => { emit('sl-load'); setTimeout(() => svg = new SVGSVGElement(), 1); }, 100);
  if (scenario === 'error') emit('sl-error');
  if (scenario === 'zero') { svg = new SVGSVGElement(); svg.getBoundingClientRect = () => ({width:0,height:16}); emit('sl-load'); }
  if (scenario === 'no-event') svg = new SVGSVGElement();
  if (scenario === 'no-dom') emit('sl-load');
  if (scenario === 'wrong-type') { svg = {namespaceURI:'http://www.w3.org/2000/svg', querySelector:()=>({}), getBoundingClientRect:()=>({width:16,height:16})}; emit('sl-load'); }
  if (scenario === 'mount-throws') throw new Error('mount failed');
}).then(() => outcome = 'ready', e => outcome = e.message);
(async () => {
  await Promise.resolve();
  while (outcome === 'pending' && tasks.size) {
    const [key, task] = [...tasks].sort((a,b) => a[1].at-b[1].at)[0];
    tasks.delete(key); clock = task.at; task.fn(); await Promise.resolve();
  }
  console.log(JSON.stringify({outcome, clock, listeners:listeners.size, timers:tasks.size}));
})();
'''


class IconReadiness(unittest.TestCase):
    def check_case(self, scenario, expected):
        result = subprocess.run(['node', '-e', HARNESS], input=json.dumps({
            'source': runner.ICON_READINESS, 'scenario': scenario}), text=True,
            capture_output=True, check=True)
        actual = json.loads(result.stdout)
        self.assertEqual(actual['outcome'], expected)
        self.assertEqual(actual['listeners'], 0)
        self.assertEqual(actual['timers'], 0)
        return actual

    def test_cached_event_before_render(self):
        self.assertEqual(self.check_case('cached', 'ready')['clock'], 16)

    def test_cold_async_load_before_render(self):
        self.assertLess(self.check_case('cold', 'ready')['clock'], 10000)

    def test_error_is_distinct_and_cleans_up(self):
        self.check_case('error', 'icon failed: type-bold')

    def test_event_without_render_cannot_pass(self):
        self.assertEqual(self.check_case('no-dom', 'icon timeout: type-bold')['clock'], 10000)

    def test_dom_without_event_cannot_pass(self):
        self.check_case('no-event', 'icon timeout: type-bold')

    def test_zero_geometry_cannot_pass(self):
        self.check_case('zero', 'icon timeout: type-bold')

    def test_non_svg_element_cannot_pass(self):
        self.check_case('wrong-type', 'icon timeout: type-bold')

    def test_mount_exception_cleans_up(self):
        self.check_case('mount-throws', 'mount failed')


if __name__ == '__main__':
    unittest.main()
