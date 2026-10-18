(function (root) {
  'use strict';
  const OPS = new Set(['prepare_role', 'prepare_snapshot', 'activate_role', 'clear_role', 'index_documents', 'remove_document', 'match', 'connected', 'documents_for_term', 'learning_documents', 'query', 'set_triggers', 'document', 'nodes', 'edges', 'index_learning', 'adjust_learning_rank', 'stats', 'export_snapshot']);
  const IMPORTS = new Set(['prepare_role', 'prepare_snapshot', 'activate_role', 'index_documents']);
  const decimal = value => typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value);
  class RoleGraphError extends Error {
    constructor(code, message) { super(message); this.name = 'RoleGraphError'; this.code = code; }
  }
  const error = (code, message) => new RoleGraphError(code, message || code);
  class TeRoleGraphClient {
    constructor(options = {}) {
      this.options = options;
      this.generation = '0'; this.revision = '0'; this.state = 'starting';
      this.queue = []; this.active = null; this.sequence = 0; this.epoch = 0; this.authoritativeEpoch = 0;
      this.lifecycle = Promise.resolve(); this.hasRole = false; this.checkpoint = null;
      this.listeners = new Set(); this.destroyed = false;
      if (typeof options.onStateChange === 'function') this.listeners.add(options.onStateChange);
      this.ready = this._start();
      // A host may never request the optional graph; avoid an unhandled rejection.
      this.ready.catch(() => {});
    }
    onStateChange(listener) { this.listeners.add(listener); return () => this.listeners.delete(listener); }
    _state(state) {
      this.state = state;
      for (const listener of this.listeners) { try { listener({ state, generation: this.generation, revision: this.revision }); } catch (_) {} }
    }
    _start() {
      return new Promise((resolve, reject) => {
        this.startup = { resolve, reject };
        try {
          const base = this.options.assetBase;
          if (!base && !(this.options.workerUrl && this.options.glueUrl && this.options.wasmUrl)) throw error('invalid_input', 'Explicit graph asset base or URLs required');
          const url = (override, relative) => override || new URL(relative, base).href;
          this.worker = (this.options.workerFactory || ((workerUrl, config) => new Worker(workerUrl, config)))(url(this.options.workerUrl, 'js/rolegraph-worker.js'), { type: 'module' });
          this.messageListener = event => this._receive(event.data);
          this.errorListener = () => this._fail(error('worker_failed', 'Graph worker failed'));
          this.worker.addEventListener('message', this.messageListener);
          this.worker.addEventListener('error', this.errorListener);
          this.worker.addEventListener('messageerror', this.errorListener);
          this.startTimer = setTimeout(() => this._fail(error('timeout', 'Graph worker initialization timed out')), this.options.importTimeout ?? 30000);
          this.worker.postMessage({ type: 'init', glueUrl: url(this.options.glueUrl, 'rolegraph/terraphim_rolegraph_browser.js'), wasmUrl: url(this.options.wasmUrl, 'rolegraph/terraphim_rolegraph_browser_bg.wasm') });
        } catch (_) { this._fail(error('worker_failed', 'Graph worker could not start')); }
      });
    }
    _receive(data) {
      if (this.destroyed || this.state === 'failed') return;
      if (data?.type === 'failed') { this._fail(error('worker_failed', 'Graph worker failed')); return; }
      if (data?.type === 'ready') {
        if (!this.startup) return;
        clearTimeout(this.startTimer); const startup = this.startup; this.startup = null;
        if (this.state === 'starting') this._state('ready'); startup.resolve(); this._pump(); return;
      }
      const job = this.active;
      if (!job || data?.requestId !== job.request.requestId) return;
      const generation = ['activate_role', 'clear_role'].includes(job.request.op) && data.ok === true ? job.request.payload.nextGeneration : job.request.generation;
      const activation = ['activate_role', 'clear_role'].includes(job.request.op);
      if (data.version !== 1 || data.generation !== generation || !decimal(data.revision) || typeof data.ok !== 'boolean' || (data.ok && (activation ? data.revision !== '0' : BigInt(data.revision) < BigInt(this.revision))) || (data.ok ? !Object.prototype.hasOwnProperty.call(data, 'result') : !data.error || typeof data.error.code !== 'string' || typeof data.error.message !== 'string')) {
        this._fail(error('worker_failed', 'Invalid graph response')); return;
      }
      this.active = null; this._cleanup(job);
      if (data.ok) {
        this.generation = data.generation; this.revision = data.revision;
        if (job.internal || job.epoch === this.epoch) job.resolve(data.result);
        else job.reject(error('stale_generation'));
      } else job.reject(error(data.error.code, data.error.message));
      this._pump();
    }
    _cleanup(job) { clearTimeout(job.timer); if (job.signal) job.signal.removeEventListener('abort', job.abort); }
    _stopWorker() {
      clearTimeout(this.startTimer);
      if (!this.worker) return;
      this.worker.removeEventListener('message', this.messageListener);
      this.worker.removeEventListener('error', this.errorListener);
      this.worker.removeEventListener('messageerror', this.errorListener);
      this.worker.terminate(); this.worker = null;
    }
    _fail(reason) {
      const recover = !this.destroyed && this.state === 'ready' && this.hasRole && typeof this.checkpoint === 'string';
      const previous = { generation: this.generation, hasRole: this.hasRole, checkpoint: this.checkpoint };
      this._stopWorker(); this._state(this.destroyed ? 'destroyed' : 'failed');
      if (this.startup) { this.startup.reject(reason); this.startup = null; }
      const jobs = this.active ? [this.active, ...this.queue] : this.queue;
      this.active = null; this.queue = [];
      for (const job of jobs) { this._cleanup(job); job.reject(reason); }
      if (recover) {
        const epoch = this.epoch;
        this.lifecycle = this._rollback(previous).then(() => {
          if (!this.destroyed && epoch === this.epoch && this.state !== 'failed') this._state('ready');
        });
      }
    }
    _pump() {
      if (this.active || this.startup || !this.worker || this.destroyed) return;
      const job = this.queue.shift(); if (!job) return;
      this.active = job;
      job.request.generation = this.generation;
      if (job.request.expectedRevision === undefined) job.request.expectedRevision = this.revision;
      job.timer = setTimeout(() => this._fail(error('timeout', 'Graph operation timed out')), IMPORTS.has(job.request.op) ? (this.options.importTimeout ?? 30000) : (this.options.queryTimeout ?? 5000));
      try { this.worker.postMessage(job.request); } catch (_) { this._fail(error('worker_failed', 'Graph request failed')); }
    }
    command(op, payload = {}, options = {}) { return this._command(op, payload, options, false); }
    _command(op, payload = {}, options = {}, internal = false) {
      if (this.destroyed) return Promise.reject(error('destroyed'));
      if (!OPS.has(op)) return Promise.reject(error('invalid_input', 'Unsupported graph operation'));
      if (!internal && ['prepare_role', 'prepare_snapshot', 'activate_role', 'clear_role'].includes(op)) return Promise.reject(error('invalid_input', 'Use role lifecycle APIs'));
      if (!internal && this.state === 'syncing') return Promise.reject(error('syncing'));
      if (this.state === 'failed') return Promise.reject(error('worker_failed'));
      if (this.queue.length >= 32) return Promise.reject(error('resource_limit', 'Graph queue is full'));
      if (options.signal?.aborted) return Promise.reject(error('cancelled'));
      return new Promise((resolve, reject) => {
        const job = { resolve, reject, internal, epoch: this.epoch, signal: options.signal, request: { version: 1, requestId: String(++this.sequence), generation: this.generation, expectedRevision: options.expectedRevision, op, payload } };
        job.abort = () => {
          if (this.active === job) { this._fail(error('cancelled', 'Running graph operation cancelled')); return; }
          const index = this.queue.indexOf(job); if (index < 0) return;
          this.queue.splice(index, 1); this._cleanup(job); reject(error('cancelled'));
        };
        if (job.signal) job.signal.addEventListener('abort', job.abort, { once: true });
        this.queue.push(job); this._pump();
      });
    }
    _latest(epoch) { if (this.destroyed) throw error('destroyed'); if (epoch !== this.epoch) throw error('stale_generation'); }
    _latestAuthoritative(epoch) { if (this.destroyed) throw error('destroyed'); if (epoch !== this.authoritativeEpoch) throw error('stale_generation'); }
    _transition(task, authoritative = false) {
      if (this.destroyed) return Promise.reject(error('destroyed'));
      if (this.state === 'failed' && !authoritative) return Promise.reject(error('worker_failed'));
      const epoch = ++this.epoch;
      if (authoritative) this.authoritativeEpoch = epoch;
      this._state('syncing');
      // Reject public results immediately, but drain the running worker call so
      // its real revision is reconciled before the next serialized operation.
      if (this.active && !this.active.internal) {
        this.active.reject(error('stale_generation'));
        if (this.active.signal) this.active.signal.removeEventListener('abort', this.active.abort);
      }
      // Cancel all unstarted commands at the role boundary.
      for (const job of this.queue.splice(0)) { this._cleanup(job); job.reject(error('stale_generation')); }
      const operation = this.lifecycle.catch(() => {}).then(async () => {
        if (authoritative) {
          try { await this.ready; } catch (_) { /* The authoritative task recreates a failed worker. */ }
          this._latestAuthoritative(epoch);
        } else {
          await this.ready; this._latest(epoch);
          if (!this.worker || this.state === 'failed') throw error('worker_failed');
        }
        try { return await task(epoch); }
        finally { if (epoch === this.epoch && !this.destroyed && this.state !== 'failed') this._state('ready'); }
      });
      this.lifecycle = operation; return operation;
    }
    syncRole({ generation, thesaurusText, name } = {}) {
      if (!decimal(generation) || (thesaurusText !== null && typeof thesaurusText !== 'string')) return Promise.reject(error('invalid_input'));
      return this._transition(async epoch => {
        try {
          // A preceding, now superseded synchronization may have lost its worker.
          if (!this.worker) {
            this.generation = '0'; this.revision = '0'; this.hasRole = false;
            this._state('syncing'); this.ready = this._start(); await this.ready; this._latestAuthoritative(epoch);
          }
          if (thesaurusText === null || thesaurusText === '') {
            await this._command('clear_role', { nextGeneration: generation }, {}, true);
            this.hasRole = false; this.checkpoint = null; this._latest(epoch); return;
          }
          const candidate = await this._command('prepare_role', { thesaurusText, roleName: name }, {}, true);
          this._latestAuthoritative(epoch);
          await this._command('activate_role', { token: candidate.token, nextGeneration: generation }, {}, true);
          this.hasRole = true; this.checkpoint = null; this._latest(epoch);
        } catch (reason) {
          // Main role is already authoritative: never expose the old graph.
          if (epoch === this.authoritativeEpoch && reason.code !== 'stale_generation' && !this.destroyed) this._fail(reason);
          throw reason;
        }
      }, true);
    }
    loadRole(thesaurusText, options = {}) { return this._load('prepare_role', { thesaurusText, roleName: options.name }); }
    importSnapshot(snapshotText) { return this._load('prepare_snapshot', { snapshotText }); }
    _load(op, payload) {
      return this._transition(async epoch => {
        const coordinator = this.options.roleCoordinator;
        if (!coordinator?.prepare || !coordinator?.commit || !coordinator?.discard || (coordinator.commit.constructor?.name === 'AsyncFunction' && !coordinator.allowAsyncCommit)) throw error('invalid_input', 'Role coordinator requires an atomic commit');
        const previous = { generation: this.generation, hasRole: this.hasRole, checkpoint: this.checkpoint };
        let mainToken; let activated = false; let committed = false;
        try {
          const candidate = await this._command(op, payload, {}, true); this._latest(epoch);
          // A rollback is allowed only after obtaining a real bridge checkpoint.
          if (previous.hasRole) previous.checkpoint = (await this._command('export_snapshot', {}, {}, true)).snapshotText;
          this._latest(epoch);
          mainToken = await coordinator.prepare(candidate.thesaurusText); this._latest(epoch);
          await this._command('activate_role', { token: candidate.token, nextGeneration: String(BigInt(this.generation) + 1n) }, {}, true);
          activated = true; this._latest(epoch);
          // Standalone commits remain synchronous. A native resource authority
          // explicitly opts in to an acknowledged commit; its opaque token still
          // validates the current role atomically before native publication.
          const commitResult = coordinator.commit(mainToken);
          if (commitResult && typeof commitResult.then === 'function') {
            if (!coordinator.allowAsyncCommit) throw error('invalid_input', 'Role coordinator commit must be synchronous');
            await commitResult;
          }
          committed = true; this.hasRole = true; this.checkpoint = null; this._latest(epoch);
        } catch (reason) {
          if (mainToken !== undefined) { try { await coordinator.discard(mainToken); } catch (_) {} }
          if (!committed && (activated || this.state === 'failed')) {
            // Reconcile even a superseded activation before releasing the
            // lifecycle lane. A later import must never capture its graph as
            // a main-associated baseline. New authoritative sync waits here;
            // it is serviced next without exposing this intermediate rollback.
            if (!this.destroyed) await this._rollback(previous);
          }
          throw reason instanceof RoleGraphError ? reason : error('invalid_input', 'Main role preparation or commit failed');
        }
      });
    }
    async _rollback(previous) {
      if (previous.hasRole && typeof previous.checkpoint !== 'string') { this._fail(error('worker_failed', 'No valid rollback checkpoint')); return; }
      try {
        this._stopWorker(); this.generation = '0'; this.revision = '0'; this._state('syncing'); this.ready = this._start(); await this.ready;
        if (previous.hasRole) {
          const candidate = await this._command('prepare_snapshot', { snapshotText: previous.checkpoint }, {}, true);
          await this._command('activate_role', { token: candidate.token, nextGeneration: previous.generation }, {}, true);
        } else await this._command('clear_role', { nextGeneration: previous.generation }, {}, true);
        this.hasRole = previous.hasRole; this.checkpoint = previous.checkpoint;
      } catch (_) { this._fail(error('worker_failed', 'Graph rollback failed')); }
    }
    indexDocuments(documents, options) { return this.command('index_documents', { documents }, options); }
    removeDocument(id, options) { return this.command('remove_document', { id }, options); }
    query(text, options = {}) { const { signal, expectedRevision, ...payload } = options; return this.command('query', { mode: 'graph', ...payload, text }, { signal, expectedRevision }); }
    async exportSnapshot() { return (await this.command('export_snapshot')).snapshotText; }
    setPersistence(options) {
      if (this.destroyed) throw error('destroyed');
      if (!root.TeRoleGraphStorage) throw error('storage_failed', 'Graph storage script unavailable');
      if (this.storage) this.storage.close();
      this.storage = new root.TeRoleGraphStorage(options);
    }
    async saveCheckpoint() {
      if (!this.storage) throw error('storage_failed', 'Persistence is not enabled');
      const epoch = this.epoch;
      const text = await this.exportSnapshot(); await this.storage.save(text);
      if (epoch === this.epoch && !this.destroyed) this.checkpoint = text;
      return text;
    }
    async restoreCheckpoint() {
      if (!this.storage) throw error('storage_failed', 'Persistence is not enabled');
      const epoch = this.epoch;
      const text = await this.storage.load(); if (text === null) throw error('not_found', 'No graph checkpoint');
      this._latest(epoch);
      await this.importSnapshot(text); return text;
    }
    destroy() {
      if (this.destroyed) return;
      this.destroyed = true; ++this.epoch; this._fail(error('destroyed')); this._state('destroyed');
      if (this.storage) this.storage.close(); this.listeners.clear();
    }
  }
  TeRoleGraphClient.Error = RoleGraphError;
  root.TeRoleGraphClient = TeRoleGraphClient;
})(typeof window !== 'undefined' ? window : globalThis);
