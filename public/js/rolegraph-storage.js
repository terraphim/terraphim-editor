(function (root) {
  'use strict';
  function storageError(code, message) {
    const err = new Error(message); err.name = 'RoleGraphStorageError'; err.code = code; return err;
  }
  class TeRoleGraphStorage {
    constructor({ enabled = false, namespace, indexedDB = root.indexedDB } = {}) {
      if (enabled && (typeof namespace !== 'string' || !namespace.trim())) throw storageError('invalid_input', 'Explicit persistence namespace required');
      this.enabled = enabled === true; this.namespace = namespace; this.indexedDB = indexedDB;
      this.db = null; this.opening = null; this.closed = false; this.transactions = new Set();
    }
    _open() {
      if (!this.enabled || this.closed || !this.indexedDB) return Promise.reject(storageError('storage_failed', 'Graph persistence is unavailable or disabled'));
      if (this.db) return Promise.resolve(this.db);
      if (this.opening) return this.opening;
      this.opening = new Promise((resolve, reject) => {
        let request;
        try { request = this.indexedDB.open('te-rolegraph:' + this.namespace, 1); }
        catch (_) { reject(storageError('storage_failed', 'Cannot open graph storage')); return; }
        let settled = false;
        const fail = () => { if (!settled) { settled = true; reject(storageError('storage_failed', 'Cannot open graph storage')); } };
        this.cancelOpen = fail;
        request.onupgradeneeded = () => {
          const db = request.result;
          if (!db.objectStoreNames.contains('snapshots')) db.createObjectStore('snapshots');
        };
        request.onerror = fail; request.onblocked = fail;
        request.onsuccess = () => {
          const db = request.result;
          if (settled || this.closed) { db.close(); fail(); return; }
          settled = true; this.db = db;
          db.onversionchange = () => { db.close(); if (this.db === db) this.db = null; };
          resolve(db);
        };
      }).finally(() => { this.opening = null; this.cancelOpen = null; });
      return this.opening;
    }
    async _transaction(mode, operation) {
      const db = await this._open();
      if (this.closed) throw storageError('storage_failed', 'Graph storage is closed');
      return new Promise((resolve, reject) => {
        let tx; let result = null; let settled = false;
        const finish = failure => {
          if (settled) return; settled = true; this.transactions.delete(tx);
          if (failure) reject(storageError('storage_failed', 'Graph storage transaction failed'));
          else resolve(result);
        };
        try {
          tx = db.transaction('snapshots', mode); this.transactions.add(tx);
          tx.oncomplete = () => finish(false); tx.onabort = () => finish(true); tx.onerror = () => finish(true);
          operation(tx.objectStore('snapshots'), value => { result = value; });
        } catch (_) {
          if (tx) { try { tx.abort(); } catch (_) {} } finish(true);
        }
      });
    }
    save(snapshotText) {
      if (typeof snapshotText !== 'string' || new TextEncoder().encode(snapshotText).length > 16 * 1024 * 1024) return Promise.reject(storageError('invalid_input', 'Snapshot text exceeds storage limit'));
      // One atomic overwrite; success means transaction commit, not put success.
      return this._transaction('readwrite', store => store.put(snapshotText, 'checkpoint:v1'));
    }
    load() {
      return this._transaction('readonly', (store, result) => {
        const request = store.get('checkpoint:v1');
        request.onsuccess = () => result(request.result === undefined ? null : request.result);
      }).then(text => {
        if (text !== null && typeof text !== 'string') throw storageError('storage_failed', 'Invalid stored snapshot text');
        // Actual validation belongs to client.importSnapshot / Rust, not JS parsing.
        return text;
      });
    }
    close() {
      if (this.closed) return;
      this.closed = true; if (this.cancelOpen) this.cancelOpen();
      for (const tx of this.transactions) { try { tx.abort(); } catch (_) {} }
      if (this.db) this.db.close(); this.db = null;
      // Never delete the database or the user's opted-in checkpoint on teardown.
    }
  }
  root.TeRoleGraphStorage = TeRoleGraphStorage;
})(typeof window !== 'undefined' ? window : globalThis);
