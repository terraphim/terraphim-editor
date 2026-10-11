/* Host-neutral synchronization. No DOM, filesystem, network or host globals.
 * The injected engine is a real instance-owned TerraphimHostSession binding.
 */
(function (root) {
  'use strict';
  class TerraphimHostBridge {
    constructor({ engine, send, adopt, conflict }) {
      this.engine = engine;
      this.send = send;
      this.adopt = adopt;
      this.conflict = conflict;
      this.current = JSON.parse(engine.snapshot());
      this.latest = this.current.source;
      this.sequence = 0;
      this.pending = null;
      this.paused = false;
      this.disposed = false;
      this.barriers = [];
      this.commandPending = null;
    }
    static ownsHistory({ inSurface = false, alternative = false, isNew = false, value, committed } = {}) {
      return inSurface || (alternative && !isNew && typeof committed === 'string' && value === committed);
    }
    exportSnapshot(message) {
      if (this.disposed || this.paused || message.protocol !== 1
          || message.resource !== this.current.resource || message.session !== this.current.session
          || message.id !== this.commandPending || message.revision !== this.current.revision
          || message.sourceHash !== this.current.sourceHash) throw new Error('Stale export request');
      return this.current.projection.export;
    }
    submit(source) {
      if (this.disposed || this.paused) throw new Error('Host session is not editable');
      if (typeof source !== 'string') throw new TypeError('Source must be text');
      this.latest = source;
      this.flush();
    }
    flush() {
      if (this.disposed || this.paused || this.pending || this.commandPending) return;
      if (this.latest === this.current.source) {
        if (!this.barriers.length) return;
        this.commandPending = ++this.sequence;
        this.send({
          protocol: 1, type: this.barriers.shift(), id: this.commandPending, resource: this.current.resource,
          session: this.current.session, revision: this.current.revision,
          sourceHash: this.current.sourceHash,
        });
        return;
      }
      const input = this.latest;
      const proposal = JSON.parse(this.engine.propose(JSON.stringify({
        protocol: 1, resource: this.current.resource, session: this.current.session,
        revision: this.current.revision, sourceHash: this.current.sourceHash,
        id: ++this.sequence, command: 'replace_source', args: { source: input },
      })));
      this.pending = { proposal, input };
      // A canonicalized no-op needs no native edit or artificial revision.
      if (proposal.source === this.current.source) {
        this.engine.reject(proposal.id);
        this.pending = null;
        if (this.latest === input) this.latest = proposal.source;
        this.adopt(proposal.source, { canonical: true, projection: proposal.projection });
        this.flush();
        return;
      }
      this.send({ ...proposal, type: 'proposal' });
    }
    command(type) {
      if (!['undo', 'redo', 'save', 'open', 'export'].includes(type)) throw new Error('Unsupported host command');
      if (this.disposed || this.paused) throw new Error('Host session is not editable');
      this.barriers.push(type);
      this.flush();
    }
    receive(message) {
      if (this.disposed || message.protocol !== 1 || message.resource !== this.current.resource
          || message.session !== this.current.session) return false;
      if (message.type === 'command-complete') {
        if (message.id !== this.commandPending) return false;
        if (message.revision > this.current.revision) {
          this.receive({ ...message, type: 'sync' });
        } else if (message.revision !== this.current.revision || message.source !== this.current.source) {
          throw new Error('Invalid command completion');
        }
        this.commandPending = null;
        this.flush();
        return true;
      }
      if (message.type === 'ack') {
        if (!this.pending || message.id !== this.pending.proposal.id) return false;
        const { proposal, input } = this.pending;
        // The engine checks exact actual source and an advancing host revision.
        this.current = JSON.parse(this.engine.acknowledge(message.id, message.revision, message.source));
        this.pending = null;
        if (this.latest === input) {
          this.latest = this.current.source;
          if (input !== proposal.source) this.adopt(this.current.source, { canonical: true, projection: this.current.projection });
        }
        this.flush();
        return true;
      }
      if (message.type !== 'sync' && message.type !== 'reject') return false;
      if (message.type === 'reject' && message.id !== this.pending?.proposal.id && message.id !== this.commandPending) {
        // An obsolete refusal must never cancel a newer operation. A newer
        // authoritative snapshot still follows the normal external-sync path.
        if (message.revision > this.current.revision) return this.receive({ ...message, type: 'sync' });
        return false;
      }

      if (message.type === 'sync' && message.revision <= this.current.revision) return false;
      if (message.type === 'sync' && this.pending
          && message.originSession === this.current.session
          && message.originId === this.pending.proposal.id
          && message.source === this.pending.proposal.source) return true;
      if (message.type === 'reject' && message.id === this.commandPending) {
        this.commandPending = null;
        this.barriers = [];
      }
      const dirty = !!this.pending || this.latest !== this.current.source;
      const draft = this.latest;
      if (this.pending) this.engine.reject(this.pending.proposal.id);
      this.pending = null;
      if (message.revision > this.current.revision) {
        this.current = JSON.parse(this.engine.synchronize(message.revision, message.source));
      } else if (message.source !== this.current.source) {
        throw new Error('Host changed source without advancing revision');
      }
      this.latest = this.current.source;
      if (!this.commandPending) this.barriers = [];
      if (dirty) {
        this.barriers = [];
        this.paused = true;
        this.conflict({ draft, source: this.current.source, reason: message.reason || 'External document change' });
      } else this.adopt(this.current.source, { external: true });
      return true;
    }
    resume() {
      if (this.disposed) throw new Error('Host session is disposed');
      this.paused = false;
      this.adopt(this.current.source, { recovery: true });
    }
    dispose() {
      if (this.disposed) return;
      this.disposed = true;
      if (this.pending) this.engine.reject(this.pending.proposal.id);
      this.pending = null;
      this.barriers = [];
      if (typeof this.engine.free === 'function') this.engine.free();
    }
  }
  if (typeof module === 'object' && module.exports) module.exports = { TerraphimHostBridge };
  else root.TerraphimHostBridge = TerraphimHostBridge;
})(typeof window === 'object' ? window : globalThis);
