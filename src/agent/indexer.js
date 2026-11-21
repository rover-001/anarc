// Anarc in-page indexer: runs in isolated world, walks DOM, extracts interactive refs.
(function() {
  function isVisible(el) {
    if (!el || el.nodeType !== 1) return false;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    if (el.closest && el.closest('[aria-hidden="true"]')) return false;
    return true;
  }

  function isInViewport(rect) {
    const vh = window.innerHeight || document.documentElement.clientHeight;
    const vw = window.innerWidth || document.documentElement.clientWidth;
    return rect.top < vh && rect.bottom > 0 && rect.left < vw && rect.right > 0;
  }

  function getAccessibleName(el) {
    let name = el.getAttribute('aria-label');
    if (name) return name.trim().slice(0, 40);

    const labelledBy = el.getAttribute('aria-labelledby');
    if (labelledBy) {
      const labelEl = document.getElementById(labelledBy);
      if (labelEl && labelEl.innerText) return labelEl.innerText.trim().slice(0, 40);
    }

    if (el.id) {
      const label = document.querySelector(`label[for="${el.id}"]`);
      if (label && label.innerText) return label.innerText.trim().slice(0, 40);
    }
    const parentLabel = el.closest ? el.closest('label') : null;
    if (parentLabel && parentLabel.innerText) {
      return parentLabel.innerText.trim().slice(0, 40);
    }

    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable) {
      if (el.getAttribute('aria-label')) return el.getAttribute('aria-label').trim().slice(0, 40);
      if (el.placeholder) return el.placeholder.trim().slice(0, 40);
      if (el.getAttribute('data-placeholder')) return el.getAttribute('data-placeholder').trim().slice(0, 40);
      if (el.value && el.type !== 'password') return el.value.trim().slice(0, 40);
    }

    if (el.innerText && el.innerText.trim()) {
      return el.innerText.trim().replace(/\s+/g, ' ').slice(0, 40);
    }

    if (el.title) return el.title.trim().slice(0, 40);
    if (el.getAttribute('alt')) return el.getAttribute('alt').trim().slice(0, 40);
    if (el.getAttribute('src')) return el.getAttribute('src').split('/').pop().slice(0, 40);

    return '';
  }

  function getKind(el) {
    const tag = el.tagName.toLowerCase();
    const role = (el.getAttribute('role') || '').toLowerCase();

    if (tag === 'input') {
      const type = (el.getAttribute('type') || 'text').toLowerCase();
      if (type === 'checkbox' || type === 'radio') return 'c';
      if (type === 'file') return 'f';
      if (type === 'submit' || type === 'button' || type === 'reset') return 'b';
      return 'i';
    }
    if (tag === 'textarea' || el.isContentEditable || el.hasAttribute('contenteditable') || role === 'textbox') return 'i';
    if (tag === 'button') return 'b';
    if (tag === 'a' && el.hasAttribute('href')) return 'l';
    if (tag === 'select') return 's';
    if (tag === 'option') return 'o';
    if (tag === 'canvas' || tag === 'svg' || tag === 'video' || tag === 'img' || tag === 'iframe') return 'm';

    if (role === 'button') return 'b';
    if (role === 'link') return 'l';
    if (role === 'checkbox' || role === 'radio' || role === 'switch') return 'c';
    if (role === 'combobox' || role === 'listbox') return 's';
    if (role === 'option' || role === 'menuitem' || role === 'tab') return 'o';

    const style = window.getComputedStyle(el);
    if (style.cursor === 'pointer' || el.hasAttribute('onclick') || el.tabIndex >= 0) {
      return 'x';
    }
    return null;
  }

  function getFlags(el, kind) {
    const flags = [];
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute('type') || '').toLowerCase();

    if (tag === 'canvas') flags.push('canvas');
    if (tag === 'iframe') flags.push('iframe');

    if (type === 'password') flags.push('secret');
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') flags.push('disabled');
    if (el.required || el.getAttribute('aria-required') === 'true') flags.push('req');

    const isInvalid = el.getAttribute('aria-invalid') === 'true' ||
      (el.classList && (el.classList.contains('error') || el.classList.contains('is-invalid')));
    if (isInvalid) flags.push('err');

    if (kind === 'c') {
      flags.push(el.checked ? 'on' : 'off');
    }

    if (document.activeElement === el) flags.push('focus');

    if (kind === 'b') {
      const cls = (el.className || '').toString().toLowerCase();
      if (cls.includes('primary') || type === 'submit') flags.push('primary');
    }

    return flags;
  }

  function getPath(el) {
    const parts = [];
    let cur = el;
    while (cur && cur.nodeType === 1 && cur !== document.body && parts.length < 5) {
      let part = cur.tagName.toLowerCase();
      if (cur.id) {
        part += '#' + cur.id;
        parts.unshift(part);
        break;
      } else if (cur.classList && cur.classList.length > 0) {
        part += '.' + Array.from(cur.classList).slice(0, 2).join('.');
      }
      parts.unshift(part);
      cur = cur.parentElement;
    }
    return parts.join('>');
  }

  function collectElementsWithShadow(root, out) {
    if (!root) return;
    const candidates = root.querySelectorAll('input, button, select, textarea, [contenteditable], [role="textbox"], rich-textarea, a[href], [role], [tabindex], [onclick], canvas, svg, video, img, iframe');
    for (let i = 0; i < candidates.length; i++) {
      out.push(candidates[i]);
      if (candidates[i].shadowRoot) {
        collectElementsWithShadow(candidates[i].shadowRoot, out);
      }
    }
  }

  window.__anarc_index = function(scope) {
    scope = scope || 'view';
    const all = [];
    const elements = [];
    collectElementsWithShadow(document, elements);

    for (let i = 0; i < elements.length; i++) {
      const el = elements[i];
      if (!isVisible(el)) continue;

      const rect = el.getBoundingClientRect();
      if (scope === 'view' && !isInViewport(rect)) continue;

      const kind = getKind(el);
      if (!kind) continue;

      const name = getAccessibleName(el);
      const flags = getFlags(el, kind);
      const path = getPath(el);
      const isBanner = Boolean(el.closest && el.closest('[role="banner"], [role="alert"], .cookie-banner, #cookie-banner, .banner, .toast, .alert'));
      const isDialog = Boolean(el.closest && el.closest('dialog, [role="dialog"], [role="alertdialog"], .modal'));

      all.push({
        kind,
        name,
        flags,
        x: Math.round(rect.left + rect.width / 2),
        y: Math.round(rect.top + rect.height / 2),
        box: [Math.round(rect.left), Math.round(rect.top), Math.round(rect.width), Math.round(rect.height)],
        path,
        is_banner: isBanner,
        is_dialog: isDialog
      });
    }

    const scrollY = window.scrollY || document.documentElement.scrollTop || 0;
    const scrollMax = Math.max(0, (document.documentElement.scrollHeight || document.body.scrollHeight) - window.innerHeight);
    const scrollRemaining = Math.max(0, Math.round(scrollMax - scrollY));

    return JSON.stringify({
      title: document.title || '',
      url: location.href || '',
      scroll_remaining: scrollRemaining,
      elements: all
    });
  };

  function lastVisibleText(selector) {
    const candidates = document.querySelectorAll(selector);
    for (let i = candidates.length - 1; i >= 0; i--) {
      const el = candidates.item(i);
      if (isVisible(el) && el.innerText?.trim()) return el.innerText.trim();
    }
    return '';
  }

  function lastMessage() {
    return lastVisibleText('article, [role=article], [data-message], [data-message-author-role]')
      || lastVisibleText('main > div:last-child, [role=main] > div:last-child');
  }

  window.__anarc_wait_stable = async function(ms) {
    const start = performance.now();
    let lastChange = start;
    const length = () => (lastMessage() || (document.querySelector('main,[role=main]') || document.body)?.innerText || '').length;
    let previous = length();
    while (true) {
      const now = performance.now();
      const current = length();
      if (current !== previous) {
        previous = current;
        lastChange = now;
      }
      if (now - start >= 30000) return 'streaming';
      if (now - lastChange >= ms) return window.__anarc_index('view');
      await new Promise(resolve => setTimeout(resolve, Math.min(100, ms, 30000 - (now - start))));
    }
  };

  window.__anarc_read = function(mode, maxChars) {
    mode = mode || 'text';
    maxChars = maxChars || 4000;

    const last = mode === 'last' ? lastMessage() : '';
    if (last) return last.slice(0, maxChars);
    if (mode === 'outline') {
      const headings = Array.from(document.querySelectorAll('h1, h2, h3, h4, h5, h6'))
        .filter(isVisible)
        .map(h => {
          const level = parseInt(h.tagName.substring(1), 10);
          const indent = '  '.repeat(Math.max(0, level - 1));
          return `${indent}h${level}: ${h.innerText.trim()}`;
        });
      return headings.join('\n');
    }

    if (mode === 'links') {
      const links = Array.from(document.querySelectorAll('a[href]'))
        .filter(isVisible)
        .map(a => `${a.innerText.trim() || 'unnamed'} -> ${a.getAttribute('href')}`)
        .filter(l => l.length > 4);
      return links.slice(0, 100).join('\n');
    }

    if (mode === 'table') {
      const tables = Array.from(document.querySelectorAll('table')).filter(isVisible);
      const lines = [];
      for (const t of tables) {
        const rows = Array.from(t.querySelectorAll('tr'));
        for (const r of rows) {
          const cells = Array.from(r.querySelectorAll('th, td')).map(c => c.innerText.trim().replace(/\t/g, ' '));
          lines.push(cells.join('\t'));
        }
        lines.push('---');
      }
      return lines.join('\n');
    }

    // Default 'text' mode: readability-style extraction
    const articles = document.querySelectorAll('article, [data-message-author-role="assistant"], [data-message-author-role="user"]');
    let text = '';
    if (articles.length > 0) {
      const parts = [];
      articles.forEach(art => {
        const clone = art.cloneNode(true);
        const remove = clone.querySelectorAll('script, style, noscript, svg, nav, footer, header, button');
        remove.forEach(n => n.remove());
        const t = (clone.innerText || clone.textContent || '').trim();
        if (t) parts.push(t);
      });
      text = parts.join('\n\n---\n\n');
    } else {
      const main = document.querySelector('main, [role="main"]') || document.body;
      const clone = main.cloneNode(true);
      const remove = clone.querySelectorAll('script, style, noscript, svg, nav, footer, header, button');
      remove.forEach(n => n.remove());
      text = clone.innerText || clone.textContent || '';
    }
    text = text.replace(/[ \t]+/g, ' ').replace(/\n\s*\n+/g, '\n\n').trim();
    if (mode === 'last') {
      // Approximate the final 800 tokens using the same 3.5 bytes/token as telemetry.
      const bytes = new TextEncoder().encode(text);
      return new TextDecoder().decode(bytes.slice(-Math.min(maxChars, 2800))).replace(/^�/, '');
    }
    if (text.length > maxChars) {
      text = text.slice(0, maxChars) + `\n… (trimmed to ${maxChars} chars)`;
    }
    return text;
  };
})();
