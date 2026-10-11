"""Offline harness unit checks. No browser, build, capture or download."""
import importlib.util
import contextlib
import json
import pathlib
import tempfile
import unittest
import subprocess
from argparse import Namespace
from datetime import date
from unittest.mock import patch
from unittest.mock import Mock
import browser_demo_common as common
from browser_demo_common import PROMPTS, ROOT, safe_name, verified_assets, verify_imports, verify_probe

spec = importlib.util.spec_from_file_location('safari_recording', ROOT / 'demos/record-safari.py')
safari = importlib.util.module_from_spec(spec)
spec.loader.exec_module(safari)


class RecordingTests(unittest.TestCase):
    def test_discrete_drag_stays_in_viewport_and_reaches_exact_target(self):
        for start, end in [([900, 600], [100, 200]), ([10, 20], [1139, 719])]:
            actions = safari.safari_drag_actions(start, end)
            points = [a for a in actions if a['type'] == 'pointerMove']
            self.assertEqual((points[-1]['x'], points[-1]['y']), tuple(end))
            for point in points:
                self.assertEqual(point['duration'], 0)
                self.assertTrue(0 <= point['x'] < 1140 and 0 <= point['y'] < 720)
            self.assertEqual(actions[-1], {'type': 'pointerUp', 'button': 0})

    def test_native_drag_geometry_rejects_invalid_coordinates(self):
        safari.native_drag_geometry([10.5, 20], [100, 200])
        for point in [[-1, 0], [1140, 0], [0, 720], [True, 1], [float('nan'), 1], [float('inf'), 1], [1], 'xy']:
            with self.assertRaises(ValueError): safari.native_drag_geometry(point, [1, 1])

    def test_discrete_drag_rejects_rounding_outside_viewport(self):
        for point in ([1139.9, 20], [10, 719.9]):
            safari.native_drag_geometry(point, [1, 1])
            with self.assertRaises(ValueError):
                safari.safari_drag_actions(point, [1, 1])
            with self.assertRaises(ValueError):
                safari.safari_drag_actions([1, 1], point)

    def test_native_drag_ack_is_exact_bounded_regular_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / 'ack.json'
            value = {'sequence': 1, 'token': 'fresh', 'performedBy': 'CUA'}
            path.write_text(json.dumps(value))
            safari.native_drag_ack(path, 1, 'fresh')
            for bad in [dict(value, token='stale'), dict(value, sequence=2), dict(value, performedBy='other'), dict(value, extra=True)]:
                path.write_text(json.dumps(bad))
                with self.assertRaises(RuntimeError): safari.native_drag_ack(path, 1, 'fresh')
            path.write_text(' ' * 4097)
            with self.assertRaises(RuntimeError): safari.native_drag_ack(path, 1, 'fresh')
            link = pathlib.Path(directory) / 'link.json'
            link.symlink_to(path)
            with self.assertRaises(RuntimeError): safari.native_drag_ack(link, 1, 'fresh')
            with self.assertRaises(RuntimeError): safari.native_drag_ack(path.parent, 1, 'fresh')

    def test_safari_context_point_inside_visible_selection(self):
        self.assertEqual(safari.selection_context_point([{'left': 10, 'top': 20, 'right': 110, 'bottom': 40}]), {'x': 18, 'y': 30})
        self.assertEqual(safari.selection_context_point([{'left': -20, 'top': -10, 'right': 12, 'bottom': 10}]), {'x': 6, 'y': 5})
        self.assertEqual(safari.selection_context_point([{'left': 10.2, 'top': 20.2, 'right': 11.5, 'bottom': 22}]), {'x': 11, 'y': 21})

    def test_safari_context_point_skips_invisible_and_rejects_bad_geometry(self):
        hidden = {'left': 1200, 'top': 20, 'right': 1300, 'bottom': 40}
        visible = {'left': 100, 'top': 100, 'right': 120, 'bottom': 120}
        self.assertEqual(safari.selection_context_point([hidden, visible]), {'x': 108, 'y': 110})
        for rects in [[], [hidden], [{'left': 0.2, 'top': 0.2, 'right': 0.4, 'bottom': 0.4}]]:
            with self.assertRaisesRegex(ValueError, 'No visible'):
                safari.selection_context_point(rects)
        for value in [float('nan'), float('inf'), True, '10', None]:
            with self.assertRaisesRegex(ValueError, 'Malformed'):
                safari.selection_context_point([dict(visible, left=value)])

    def test_safari_visibility_sequence_and_private_state(self):
        driver = Mock()
        driver.call.side_effect = ['owned-private-handle', 'owned-private-handle',
                                   {'visible': False, 'focused': True}, 'owned-private-handle',
                                   'owned-private-handle', {'visible': True, 'focused': True},
                                   'owned-private-handle']
        clock = [0.0]
        metadata = {}
        with patch.object(safari.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(safari.time, 'sleep', side_effect=lambda seconds: clock.__setitem__(0, clock[0] + seconds)), \
             patch('builtins.print') as notice:
            safari.wait_visible_owned_window(driver, metadata)
        notice.assert_called_once_with(safari.VISIBILITY_NOTICE, flush=True)
        self.assertEqual([call.args[1] for call in driver.call.call_args_list],
                         ['/window', '/window', '/execute/sync', '/window',
                          '/window', '/execute/sync', '/window'])
        self.assertEqual(metadata['visibilityPrecondition']['attempts'], 2)
        self.assertEqual(metadata['visibilityPrecondition']['status'], 'ready')
        self.assertNotIn('owned-private-handle', json.dumps(metadata))
        for call in driver.call.call_args_list:
            self.assertTrue(0 < call.kwargs['timeout'] <= 10)

    def test_safari_visibility_exhaustion_and_unavailable(self):
        for mode in ['hidden', 'unfocused', 'changed', 'unavailable', 'malformed', 'late']:
            with self.subTest(mode=mode):
                clock = [0.0]
                driver = Mock()
                def call(method, route, payload=None, timeout=20):
                    if mode == 'unavailable':
                        raise OSError('private URL and handle must not leak')
                    if route == '/window':
                        return 'other' if mode == 'changed' and driver.call.call_count > 1 else 'owned'
                    if mode == 'late':
                        clock[0] = 10.01
                    if mode == 'malformed':
                        return {'visible': 'true', 'focused': True}
                    return {'visible': mode != 'hidden', 'focused': mode != 'unfocused'}
                driver.call.side_effect = call
                metadata = {}
                with patch.object(safari.time, 'monotonic', side_effect=lambda: clock[0]), \
                     patch.object(safari.time, 'sleep', side_effect=lambda seconds: clock.__setitem__(0, clock[0] + seconds)), \
                     patch('builtins.print'), self.assertRaisesRegex(RuntimeError, 'Safari owned automation window'):
                    safari.wait_visible_owned_window(driver, metadata)
                self.assertEqual(metadata['visibilityPrecondition']['status'], 'failed')
                self.assertLessEqual(clock[0], 10.01)
                self.assertNotIn('private URL', json.dumps(metadata))
                driver.capture.assert_not_called()

    def test_safari_guard_before_capture_and_delegate_cleanup(self):
        for fail in [False, True]:
            with self.subTest(fail=fail), self.lifecycle_repo() as (_, args, _):
                args.output = args.output.with_name(args.output.name.replace('chrome-', 'safari-'))
                args.duration = 64
                args.native_drag_handoff = None
                args.record_failed_drag_chapters = False
                parser = Mock()
                parser.parse_args.return_value = args
                process = Mock()
                driver = Mock()
                driver.evaluate.side_effect = [common.VIEWPORT, common.VIEWPORT, 'unit-agent']
                driver.samples = [{'name': 'unit-a', 'completed': 0}, {'name': 'unit-b', 'completed': .25}]
                driver.context_menu_inputs = []
                driver.native_drag_inputs = []
                driver.drag_diagnostics = []
                events = []
                def guard(delegate, metadata):
                    self.assertIs(delegate, driver)
                    events.append('guard')
                    if fail:
                        raise RuntimeError('Safari owned automation window unavailable')
                driver.capture.side_effect = lambda: events.append('capture')
                def story(*_):
                    events.append('storyboard')
                def encode(*values):
                    values[-2].write_bytes(b'UNIT SAFARI OUTPUT: NOT MEDIA')
                original_popen = subprocess.Popen
                original_is_file = pathlib.Path.is_file
                def launch(command, *values, **kwargs):
                    if command[0] == '/usr/bin/safaridriver':
                        return process
                    return original_popen(command, *values, **kwargs)
                with patch.object(safari, 'parser', return_value=parser), \
                     patch.object(pathlib.Path, 'is_file', lambda path: str(path) == '/usr/bin/safaridriver' or original_is_file(path)), \
                     patch.object(safari.subprocess, 'Popen', side_effect=launch), \
                     patch.object(safari, 'webdriver', side_effect=[{}, {'sessionId': 'unit-owned', 'capabilities': {'browserName': 'Safari', 'platformName': 'macOS'}}, None]) as wire, \
                     patch.object(safari, 'Driver', return_value=driver), \
                     patch.object(safari, 'wait_ready'), \
                     patch.object(safari, 'wait_visible_owned_window', side_effect=guard), \
                     patch.object(safari, 'storyboard', side_effect=story), \
                     patch.object(safari, 'encode', side_effect=encode):
                    if fail:
                        with self.assertRaises(RuntimeError):
                            safari.main()
                    else:
                        safari.main()
                self.assertEqual(events, ['guard'] if fail else ['guard', 'capture', 'storyboard', 'capture'])
                self.assertEqual(wire.call_args.args[1:], ('DELETE', '/session/unit-owned'))
                process.terminate.assert_called_once()
                process.wait.assert_called_once_with(timeout=5)
                self.assertEqual(args.output.exists(), not fail)
                self.assertEqual(json.loads(args.output.with_suffix('.json').read_text())['status'],
                                 'incomplete' if fail else 'complete')

    @contextlib.contextmanager
    def lifecycle_repo(self):
        """Synthetic unit assets only: no browser or encoded media is involved."""
        import hashlib
        with tempfile.TemporaryDirectory(dir=common.SCRATCH, prefix='demo-unit-git-') as tmp:
            root = pathlib.Path(tmp)
            def git(*args):
                return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
            git('init', '-q')
            git('config', 'user.name', 'Synthetic Unit Test')
            git('config', 'user.email', 'synthetic@example.invalid')
            for name, content in {
                'demos/fixtures/alternatives.json': (ROOT / 'demos/fixtures/alternatives.json').read_bytes(),
                'demos/README.md': b'unit source',
                'demos/record-browser.py': b'# unit source',
                'demos/record-safari.py': b'# unit source',
                'demos/browser_demo_common.py': b'# unit source',
                'demos/test_recording.py': b'# unit source',
                # A unit-only checker seam avoids cloning the full app/vendor
                # tree. This test asserts lifecycle, not production qualification.
                'scripts/ci/rolegraph-production-check.py': b'# Synthetic lifecycle-unit checker seam only\n',
                'src/lib.rs': b'// unit source',
            }.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            git('add', '.')
            git('commit', '-qm', 'Synthetic scoped source')
            # Unrelated protected dirt must not require global cleanliness.
            (root / 'HANDOVER-protected.md').write_text('untouched')
            delivery = root / 'delivery'
            assets = {}
            for name in ['index.html', 'terraphim_editor.js', 'terraphim_editor_bg.wasm',
                         'terraphim-editor.min.js', 'terraphim-editor.min.css',
                         'js/rolegraph-worker.js', 'rolegraph/terraphim_rolegraph_browser.js',
                         'rolegraph/terraphim_rolegraph_browser_bg.wasm']:
                path = delivery / name
                path.parent.mkdir(parents=True, exist_ok=True)
                content = b'<!-- synthetic unit asset -->' if name.endswith('.html') else b'/* synthetic unit asset */'
                path.write_bytes(content)
                assets[name] = hashlib.sha256(content).hexdigest()
            (delivery / 'example.html').write_text('synthetic unit asset')
            manifest = root / 'unit-manifest.json'
            manifest.write_text(json.dumps({'schema': 1, 'sourceHead': git('rev-parse', 'HEAD'), 'assets': assets}))
            output = root / f'demos/out/chrome-alternatives-guided-demo-{date.today().isoformat()}-unit.mp4'
            args = Namespace(output=output, delivery=delivery, manifest=manifest)
            with patch.object(common, 'ROOT', root), patch.object(common, 'tools', return_value=('unused-unit-ffmpeg', 'unused-unit-ffprobe')):
                yield root, args, git

    def test_own_output_retained_after_source_postflight(self):
        with self.lifecycle_repo() as (root, args, git):
            with common.inputs(args, 'chrome') as (_, _, _, metadata, _, _):
                args.output.write_bytes(b'UNIT LIFECYCLE SENTINEL: NOT ENCODED MEDIA')
            self.assertEqual(metadata['status'], 'complete')
            self.assertEqual(args.output.read_bytes(), b'UNIT LIFECYCLE SENTINEL: NOT ENCODED MEDIA')
            self.assertEqual(json.loads(args.output.with_suffix('.json').read_text())['status'], 'complete')
            self.assertEqual(common.source_identity(args.output), metadata['sourceIdentity'])
            self.assertEqual(metadata['sourceIdentity']['excludedReservationPaths'], [
                args.output.relative_to(root).as_posix(), args.output.with_suffix('.json').relative_to(root).as_posix()])
            self.assertEqual((root / 'HANDOVER-protected.md').read_text(), 'untouched')
            with self.assertRaises(FileExistsError):
                with common.inputs(args, 'chrome'):
                    self.fail('Existing output must not yield')

    def test_source_head_and_unexpected_file_drift_abort_only_owned_output(self):
        for change in ['fixture', 'harness', 'safari', 'shared', 'readme', 'unit', 'head', 'unexpected-harness',
                       'unexpected-output', 'unexpected-fixture', 'delivery', 'manifest', 'stage']:
            with self.subTest(change=change), self.lifecycle_repo() as (root, args, git):
                unrelated = root / 'keep-me.txt'
                unrelated.write_text('protected')
                with self.assertRaises(ValueError if change == 'delivery' else RuntimeError):
                    with common.inputs(args, 'chrome') as (stage, _, _, _, _, _):
                        args.output.write_bytes(b'UNIT LIFECYCLE SENTINEL: NOT ENCODED MEDIA')
                        if change == 'head':
                            git('commit', '--allow-empty', '-qm', 'Unit HEAD drift')
                        elif change == 'manifest':
                            args.manifest.write_text('{}')
                        elif change == 'stage':
                            (stage / 'terraphim_editor.js').write_text('changed unit input')
                        else:
                            target = {'fixture': 'demos/fixtures/alternatives.json',
                                      'harness': 'demos/record-browser.py', 'readme': 'demos/README.md',
                                      'safari': 'demos/record-safari.py', 'shared': 'demos/browser_demo_common.py',
                                      'unit': 'demos/test_recording.py', 'unexpected-harness': 'demos/unexpected.py',
                                      'unexpected-output': 'demos/out/unrelated.mp4',
                                      'unexpected-fixture': 'demos/fixtures/unexpected.json',
                                      'delivery': 'delivery/terraphim_editor.js'}[change]
                            (root / target).write_text('changed unit input')
                self.assertFalse(args.output.exists())
                self.assertEqual(json.loads(args.output.with_suffix('.json').read_text())['status'], 'incomplete')
                self.assertEqual(unrelated.read_text(), 'protected')
                if change == 'unexpected-output':
                    self.assertEqual((root / 'demos/out/unrelated.mp4').read_text(), 'changed unit input')

    def test_symlink_output_backdoor_refused(self):
        with self.lifecycle_repo() as (root, args, git):
            outside = root / 'protected-output'
            outside.mkdir()
            args.output.parent.symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError):
                with common.inputs(args, 'chrome'):
                    self.fail('Symlinked output directory must not yield')
            self.assertEqual(list(outside.iterdir()), [])

    def test_replaced_output_does_not_delete_unrelated_target(self):
        with self.lifecycle_repo() as (root, args, git):
            protected = root / 'protected-target'
            protected.write_text('never change or delete')
            with self.assertRaises(ValueError):
                with common.inputs(args, 'chrome'):
                    args.output.unlink()  # Unit-only replacement of own reservation.
                    args.output.symlink_to(protected)
            self.assertTrue(args.output.is_symlink())
            self.assertEqual(protected.read_text(), 'never change or delete')
            self.assertEqual(json.loads(args.output.with_suffix('.json').read_text())['status'], 'incomplete')

    def test_encoder_symlink_identity_discloses_system_target(self):
        import hashlib
        with tempfile.TemporaryDirectory(dir=common.SCRATCH, prefix='demo-unit-encoder-') as tmp:
            target = pathlib.Path(tmp) / 'system-encoder'
            target.write_bytes(b'UNIT IDENTITY ONLY: NOT AN EXECUTABLE')
            registry = pathlib.Path(tmp) / 'registry-encoder'
            registry.symlink_to(target)
            with patch.object(common, 'run', return_value='ffmpeg version unit-test\n'):
                identity = common.encoder_identity(registry)
            self.assertEqual(identity['registryPath'], str(registry))
            self.assertEqual(identity['resolvedTarget'], str(target))
            self.assertEqual(identity['binarySha256'], hashlib.sha256(target.read_bytes()).hexdigest())
            self.assertEqual(identity['version'], 'ffmpeg version unit-test')
            self.assertEqual(identity['provenance'], 'externally supplied system encoder via symlink')

    def test_manifest_paths(self):
        for bad in ['../private', '/tmp/a', 'js/../a', 'js\\a', 'a//b', './a', '']:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                safe_name(bad)
        self.assertEqual(str(safe_name('vendor/snippets/a.js')), 'vendor/snippets/a.js')

    def test_incomplete_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                verified_assets(pathlib.Path(tmp), {'schema': 1, 'assets': {}})

    def test_verified_manifest_and_drift(self):
        import hashlib
        names = ['index.html', 'terraphim_editor.js', 'terraphim_editor_bg.wasm',
                 'terraphim-editor.min.js', 'terraphim-editor.min.css', 'js/rolegraph-worker.js',
                 'rolegraph/terraphim_rolegraph_browser.js',
                 'rolegraph/terraphim_rolegraph_browser_bg.wasm', 'vendor/LICENSE',
                 'vendor/fontawesome/6.5.2/webfonts/fa-solid-900.ttf']
        with tempfile.TemporaryDirectory() as tmp:
            delivery = pathlib.Path(tmp)
            assets = {}
            for name in names:
                p = delivery / name
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(name.encode())
                assets[name] = hashlib.sha256(name.encode()).hexdigest()
            manifest = {'schema': 1, 'assets': assets}
            self.assertEqual(list(verified_assets(delivery, manifest)), names)
            (delivery / names[1]).write_bytes(b'changed')
            with self.assertRaises(ValueError):
                verified_assets(delivery, manifest)

    def test_no_test_html(self):
        # Path validation must reject test evidence even with a supplied hash.
        source = (ROOT / 'demos/browser_demo_common.py').read_text()
        self.assertIn("p.suffix == '.html' and name != 'index.html'", source)

    def test_recursive_import_closure(self):
        assets = {'index.html': b'<script src="glue.js"></script>',
                  'glue.js': b'import "./snippets/one.js";',
                  'snippets/one.js': b'export {x} from "./two.js";',
                  'snippets/two.js': b'export const x=1;'}
        verify_imports(assets)
        del assets['snippets/two.js']
        with self.assertRaises(ValueError):
            verify_imports(assets)
        with self.assertRaises(ValueError):
            verify_imports({'index.html': b'<link href="https://cdn.invalid/a.css">'})

    def test_fixture_and_storyboard(self):
        fixture = json.loads((ROOT / 'demos/fixtures/alternatives.json').read_text())
        self.assertEqual(fixture['id'], 'unicode-local-alternatives-v1')
        self.assertEqual(list(fixture['role']['data']), ['rubber', 'eraser'])
        self.assertEqual([v['id'] for v in fixture['role']['data'].values()], [1, 1])
        self.assertTrue(fixture['body'].endswith(fixture['term']))
        self.assertGreater(len(fixture['body'].encode('utf-16-le')) // 2, len(fixture['body']))
        self.assertTrue(3 <= len(PROMPTS) <= 8)
        readme = (ROOT / 'demos/README.md').read_text()
        for i, text in enumerate(PROMPTS, 1):
            self.assertLessEqual(len(text), 80)
            self.assertIn(f'{i}/{len(PROMPTS)} {text}', readme)

    def test_actual_safari_only(self):
        safari.validate_capabilities({'browserName': 'Safari', 'platformName': 'macOS'})
        for value in [{'browserName': 'webkit', 'platformName': 'mac'}, {'browserName': 'safari', 'platformName': 'ios'}, {}]:
            with self.assertRaises(RuntimeError):
                safari.validate_capabilities(value)
        self.assertEqual(safari.recording_capabilities({
            'browserName': 'Safari', 'browserVersion': 'unit', 'platformName': 'macOS',
            'sessionId': 'private', 'vendor': {'url': 'private'}, 'windowHandle': 'private'}),
            {'browserName': 'Safari', 'browserVersion': 'unit', 'platformName': 'macOS'})

    def test_safari_delegate_preserves_request_timeout(self):
        driver = safari.Driver('unit-base', 'unit-session', None)
        with patch.object(safari, 'webdriver', return_value=True) as wire:
            driver.call('GET', '/window', timeout=.25)
            wire.assert_called_once_with('unit-base', 'GET', '/session/unit-session/window', None, timeout=.25)
        with patch.object(safari, 'webdriver', return_value=True) as wire:
            driver.call('GET', '/screenshot')
            wire.assert_called_once_with('unit-base', 'GET', '/session/unit-session/screenshot', None, timeout=20)

    def test_elapsed_frame_durations(self):
        self.assertEqual(safari.frame_durations([{'completed': 1}, {'completed': 1.25}, {'completed': 2}]), [.25, .75])
        for samples in [[], [{'completed': 1}], [{'completed': 1}, {'completed': 1}]]:
            with self.assertRaises(ValueError):
                safari.frame_durations(samples)

    def test_codec_probe_fail_closed(self):
        probe = {'streams': [{'codec_type': 'video', 'codec_name': 'h264', 'pix_fmt': 'yuv420p',
                              'width': 1140, 'height': 720, 'r_frame_rate': '15/1'}],
                 'format': {'duration': '64'}}
        verify_probe(probe)
        for key, value in [('codec_name', 'vp9'), ('pix_fmt', 'yuv444p'), ('width', 1024), ('r_frame_rate', '30/1')]:
            changed = json.loads(json.dumps(probe))
            changed['streams'][0][key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                verify_probe(changed)
        probe['format']['duration'] = '59'
        with self.assertRaises(RuntimeError):
            verify_probe(probe)


if __name__ == '__main__':
    unittest.main()
