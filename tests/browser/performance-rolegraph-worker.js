// Test-only wrapper: same module instance and actual production dispatch owner.
import init from './rolegraph/terraphim_rolegraph_browser.js';
import './js/rolegraph-worker.js';

const send = self.postMessage.bind(self);
let exports;
let highwater = 0;
function observe(response) {
  const bytes = exports.memory.buffer.byteLength;
  highwater = Math.max(highwater, bytes);
  send({ type: 'performance_memory', requestId: response.requestId ?? null,
    bytes, highwater, operationCompleted: response.type !== 'ready' });
}
self.postMessage = response => {
  if (response.type === 'ready') {
    // Production initialization already completed; init returns its cached exports.
    // Do not allow a client dispatch before those exports are observable.
    init().then(value => { exports = value; observe(response); send(response); });
    return;
  }
  if (exports) observe(response); // Immediately after synchronous Rust dispatch.
  send(response);
};
