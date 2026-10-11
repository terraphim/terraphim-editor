// Module worker. Only the Rust bridge owns graph algorithms and validation.
let graph = null;
let initializing = false;
self.addEventListener('message', async ({ data }) => {
  if (data?.type === 'init') {
    if (initializing || graph) return;
    initializing = true;
    try {
      const bridge = await import(data.glueUrl);
      await bridge.default({ module_or_path: data.wasmUrl });
      graph = new bridge.BrowserRoleGraph();
      self.postMessage({ type: 'ready' });
    } catch (_) {
      self.postMessage({ type: 'failed', error: { code: 'worker_failed', message: 'Graph worker initialization failed' } });
    }
    return;
  }
  if (!graph) {
    self.postMessage({ type: 'failed', error: { code: 'worker_failed', message: 'Graph worker is unavailable' } });
    return;
  }
  try {
    // Parse only the wire envelope: snapshot/graph JSON remains opaque text.
    const response = graph.dispatch(JSON.stringify(data));
    self.postMessage(JSON.parse(response));
  } catch (_) {
    self.postMessage({ type: 'failed', error: { code: 'worker_failed', message: 'Graph dispatch failed' } });
  }
});
