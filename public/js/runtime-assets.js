/* Shared classic-script bootstrap: also safe to concatenate into the embed.
 * Styles/modules are awaited before UI use. No downloads outside the selected
 * asset base, no CDN fallback, no service worker, no host-language mutation.
 */
(() => {
  'use strict';
  if (window.TerraphimRuntimeAssets) return;
  const components = ['input', 'icon', 'button', 'button-group', 'tooltip',
    'split-panel', 'dialog', 'divider', 'badge'];
  const icons = new Set(['type-bold', 'type-italic', 'code', 'link', 'type-h1',
    'type-h2', 'type-h3', 'type-underline', 'question-circle']);
  const styles = new Map();
  let selectedBase = null;
  let promise = null;
  let owned = false;

  function resolveBase(options = {}, scriptURL = document.baseURI) {
    const base = new URL(options.assetBase || '.', options.assetBase ? document.baseURI : scriptURL);
    if (!['http:', 'https:'].includes(base.protocol) || base.username || base.password || base.search || base.hash) {
      throw new TypeError('Runtime assetBase must be an HTTP(S) directory URL without credentials, query or fragment');
    }
    if (base.origin !== new URL(document.baseURI).origin && options.allowExternalAssets !== true) {
      throw new TypeError('Cross-origin runtime assets require allowExternalAssets: true and host CSP/CORS approval');
    }
    if (!base.pathname.endsWith('/')) base.pathname += '/';
    return base.href;
  }

  function loadStyle(url) {
    const href = new URL(url, document.baseURI).href;
    if (styles.has(href)) return styles.get(href);
    const result = new Promise((resolve, reject) => {
      let link = [...document.querySelectorAll('link[rel="stylesheet"]')]
        .find((item) => item.href === href);
      if (link && link.sheet) { resolve(); return; }
      const created = !link;
      if (!link) {
        link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = href;
      }
      const timer = setTimeout(() => fail(), 15000);
      const fail = () => {
        clearTimeout(timer);
        // Failed local styles may be retried, including pre-existing links.
        link.remove();
        reject(new Error(`Runtime stylesheet unavailable: ${href}`));
      };
      link.addEventListener('load', () => { clearTimeout(timer); resolve(); }, { once: true });
      link.addEventListener('error', fail, { once: true });
      if (created) document.head.appendChild(link);
    });
    styles.set(href, result);
    result.catch(() => { if (styles.get(href) === result) styles.delete(href); });
    return result;
  }

  function iconResolver(base) {
    return (name) => typeof name === 'string' && /^[a-z0-9-]+$/.test(name) && icons.has(name)
      ? new URL(`assets/icons/${name}.svg`, base).href : '';
  }

  function load(options = {}, scriptURL = document.baseURI) {
    let base;
    try {
      base = resolveBase(options, scriptURL);
      if (selectedBase && selectedBase !== base) {
        throw new Error('Incompatible runtime assetBase: Shoelace registration is page-global');
      }
      if (!owned && components.some((name) => customElements.get(`sl-${name}`))) {
        throw new Error('Host-owned Shoelace detected; use loadShoelace: false and provide all runtime styles/icons');
      }
    } catch (error) { return Promise.reject(error); }
    if (promise) return promise;
    selectedBase = base;
    const shoelace = new URL('vendor/shoelace/2.12.0/cdn/', base).href;
    const fontawesome = new URL('vendor/fontawesome/6.5.2/css/', base).href;
    promise = (async () => {
      // Await styles first: on a CSS error no global custom elements changed.
      await Promise.all([loadStyle(new URL('themes/light.css', shoelace).href),
        ...['fontawesome', 'solid', 'regular', 'brands'].map((name) => loadStyle(new URL(`${name}.min.css`, fontawesome).href))]);
      const basePath = await import(new URL('utilities/base-path.js', shoelace).href);
      basePath.setBasePath(shoelace);
      const library = await import(new URL('utilities/icon-library.js', shoelace).href);
      library.registerIconLibrary('default', { resolver: iconResolver(shoelace) });
      owned = true;
      await Promise.all(components.map(async (name) => {
        const module = await import(new URL(`components/${name}/${name}.js`, shoelace).href);
        const registered = customElements.get(`sl-${name}`);
        // Shoelace 2.12.0 define() registers a subclass, not module.default.
        if (!registered || !(registered === module.default || registered.prototype instanceof module.default)) {
          throw new Error(`Incompatible Shoelace element sl-${name}; use loadShoelace: false for host-managed components`);
        }
      }));
      return { assetBase: base, shoelaceBase: shoelace };
    })();
    promise.catch(() => { promise = null; });
    return promise;
  }

  window.TerraphimRuntimeAssets = { resolveBase, loadStyle, iconResolver, load, ready: null };
})();
